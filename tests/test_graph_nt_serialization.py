"""Native N-Triples serialization preserves the existing RDFLib boundary."""

import io
import json
import subprocess
import sys
import warnings

import pytest
import rdflib
from rdflib import BNode, Literal, URIRef
from rdflib import Graph as ReferenceGraph
from rdflib.namespace import XSD
from rdflib.serializer import Serializer
from rdflib_turbo import Graph, StoreDataset


def exact_terms(document):
    # Compare RDF lexical identity, never Literal.eq's datatype value equality.
    with pytest.MonkeyPatch.context() as context:
        context.setattr(rdflib, "NORMALIZE_LITERALS", False)
        graph = ReferenceGraph().parse(
            data=document,
            format="nt",
            bnode_context={name: BNode(name) for name in ("subject-id", "object-id")},
        )

    def term(value):
        if isinstance(value, Literal):
            return "literal", str(value), value.datatype, value.language
        return ("blank" if isinstance(value, BNode) else "uri"), str(value)

    return {tuple(map(term, row)) for row in graph}


def graphs(scope):
    dataset = StoreDataset()
    identifier = (
        None
        if scope == "default"
        else URIRef("urn:named")
        if scope == "named"
        else BNode("graph-id")
    )
    graph = dataset.default_graph if identifier is None else dataset.graph(identifier)
    reference = ReferenceGraph()
    statements = [
        (URIRef("urn:s"), URIRef("urn:plain"), Literal("same")),
        (URIRef("urn:s"), URIRef("urn:typed"), Literal("same", datatype=XSD.string)),
        (
            URIRef("urn:s"),
            URIRef("urn:numeric"),
            Literal("01", datatype=XSD.integer, normalize=False),
        ),
        (BNode("subject-id"), URIRef("urn:language"), Literal("texte", lang="fr")),
        (URIRef("urn:s"), URIRef("urn:blank"), BNode("object-id")),
        (URIRef("urn:s"), URIRef("urn:unicode"), Literal('漢字🌍\\"\n\t\r' * 512)),
    ]
    for statement in statements:
        graph.add(statement)
        reference.add(statement)
    dataset.graph(URIRef("urn:unrelated")).add(
        (URIRef("urn:exclude"), URIRef("urn:p"), Literal("other graph"))
    )
    return graph, reference


@pytest.mark.parametrize("scope", ["default", "named", "blank"])
@pytest.mark.parametrize("format", ["nt", "nt11", "ntriples"])
@pytest.mark.parametrize("encoding", [None, "utf-8"])
def test_nt_serialization_reads_no_python_triples(monkeypatch, scope, format, encoding):
    graph, reference = graphs(scope)
    expected = reference.serialize(format=format, encoding=encoding)

    def forbidden(*args, **kwargs):
        raise AssertionError("N-Triples serialization traversed Python RDF terms")

    monkeypatch.setattr(Graph, "triples", forbidden)
    actual = graph.serialize(format=format, encoding=encoding)
    assert type(actual) is type(expected)
    assert exact_terms(actual) == exact_terms(expected)


def test_native_nt_serialization_observes_pending_transaction_only(monkeypatch):
    graph = Graph()
    statement = (URIRef("urn:s"), URIRef("urn:p"), Literal("pending"))
    reference = ReferenceGraph()
    reference.add(statement)
    expected = reference.serialize(format="nt")
    with graph.transaction() as transaction:
        transaction.add(statement)
        monkeypatch.setattr(Graph, "triples", lambda *args: pytest.fail("Python term traversal"))
        assert transaction.serialize(format="nt") == expected
        assert graph.serialize(format="nt") == ""
    assert graph.serialize(format="nt") == expected


def test_empty_graph_serializes_natively(monkeypatch):
    graph = Graph()
    monkeypatch.setattr(Graph, "triples", lambda *args: pytest.fail("Python term traversal"))
    assert graph.serialize(format="nt") == ""
    assert graph.serialize(format="nt", encoding="utf-8") == b""


@pytest.mark.parametrize(
    "options",
    [
        {"format": "turtle"},
        {"format": "nt", "encoding": "UTF-8"},
        {"format": "nt", "encoding": "ascii"},
        {"format": "nt", "base": "urn:base:"},
        {"format": "nt", "unused_option": True},
    ],
)
def test_other_serializer_options_preserve_original_plugin_policy(options):
    graph, reference = graphs("named")
    with warnings.catch_warnings(record=True) as expected_warnings:
        warnings.simplefilter("always")
        expected = reference.serialize(**options)
    with warnings.catch_warnings(record=True) as actual_warnings:
        warnings.simplefilter("always")
        actual = graph.serialize(**options)
    assert type(actual) is type(expected)
    if options["format"] == "nt":
        assert exact_terms(actual) == exact_terms(expected)
    else:
        assert set(actual.splitlines()) == set(expected.splitlines())
    assert [str(item.message) for item in actual_warnings] == [
        str(item.message) for item in expected_warnings
    ]


def test_graph_base_keeps_original_nt_warning():
    graph = Graph(base="urn:base:")
    with pytest.warns(UserWarning, match="does not support base"):
        assert graph.serialize(format="nt") == ""


def test_destination_keeps_reference_writer_and_return_value():
    graph, reference = graphs("default")
    destination = io.BytesIO()
    expected = io.BytesIO()
    with warnings.catch_warnings():
        warnings.simplefilter("ignore")
        reference.serialize(destination=expected, format="nt")
        assert graph.serialize(destination=destination, format="nt") is graph
    assert set(destination.getvalue().splitlines()) == set(expected.getvalue().splitlines())


def test_replaced_nt_plugin_remains_observable(monkeypatch):
    from rdflib import plugin

    class CustomSerializer(Serializer):
        def serialize(self, stream, base=None, encoding=None, **kwargs):
            stream.write(b"custom N-Triples plugin\n")

    original = plugin.get
    monkeypatch.setattr(
        plugin, "get", lambda name, kind: CustomSerializer if name == "nt" else original(name, kind)
    )
    assert Graph().serialize(format="nt") == "custom N-Triples plugin\n"


def test_graph_subclass_retains_custom_triple_projection():
    class ProjectedGraph(Graph):
        def triples(self, pattern):
            return iter(())

    graph = ProjectedGraph()
    graph.add((URIRef("urn:s"), URIRef("urn:p"), Literal("hidden")))
    assert graph.serialize(format="nt") == ""


def test_exact_nt_oracle_detects_lexical_and_datatype_identity_loss():
    prefix = "<urn:s> <urn:p> "
    assert exact_terms(
        prefix + '"01"^^<http://www.w3.org/2001/XMLSchema#integer> .'
    ) != exact_terms(prefix + '"1"^^<http://www.w3.org/2001/XMLSchema#integer> .')
    assert exact_terms(prefix + '"same" .') != exact_terms(
        prefix + '"same"^^<http://www.w3.org/2001/XMLSchema#string> .'
    )


def test_utf8_serialization_does_not_stage_a_wide_python_string():
    # A separate interpreter isolates allocation tracing from pytest/plugins.
    # One astral character makes a whole CPython text buffer four bytes/character;
    # bytes output must not pay that graph-sized Unicode intermediate cost.
    code = """
import gc, json, tracemalloc
from rdflib_turbo import Graph
graph = Graph().parse(data='\\n'.join(
    f'<urn:wide:s:{index}> <urn:wide:p> "value🌍{index}" .'
    for index in range(1024)), format='nt')
gc.collect()
tracemalloc.start()
payload = graph.serialize(format='nt', encoding='utf-8')
current, peak = tracemalloc.get_traced_memory()
tracemalloc.stop()
assert type(payload) is bytes
print(json.dumps({'output_bytes': len(payload), 'python_heap_peak': peak}))
"""
    result = subprocess.run(
        [sys.executable, "-I", "-B", "-c", code],
        check=True,
        capture_output=True,
        text=True,
        close_fds=False,
        timeout=30,
    )
    counts = json.loads(result.stdout)
    # bounded: two output-sized Python byte buffers plus64KiB call scaffolding.
    assert counts["python_heap_peak"] <= 2 * counts["output_bytes"] + 65536, counts
