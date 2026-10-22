"""JSON-LD parser options retain reference behavior on native transaction owners."""

import json

import pytest
from rdflib import BNode, Literal, URIRef, plugin
from rdflib import Dataset as ReferenceDataset
from rdflib import Graph as ReferenceGraph
from rdflib.compare import isomorphic
from rdflib.parser import StringInputSource
from rdflib.plugins.parsers.jsonld import JsonLDParser
from rdflib.plugins.stores.memory import Memory
from rdflib_turbo import Graph, StoreDataset
from rdflib_turbo.graph import RDFLibStore

DOCUMENT = json.dumps(
    {
        "@context": {"p": {"@id": "urn:p", "@type": "@id"}},
        "@id": "urn:s",
        "p": "urn:o",
    }
)


@pytest.mark.parametrize("version", [1.1, "1.1", 1.0])
@pytest.mark.parametrize("identifier", [URIRef("urn:graph"), BNode("graph")])
def test_version_option_matches_reference(version, identifier):
    expected = ReferenceGraph(identifier=identifier).parse(
        data=DOCUMENT, format="json-ld", version=version
    )
    actual = Graph(identifier=identifier)
    assert actual.parse(data=DOCUMENT, format="json-ld", version=version) is actual
    assert set(actual) == set(expected)


def test_context_base_and_blank_links_match_reference():
    document = json.dumps(
        [{"@id": "item", "p": {"@id": "_:child"}}, {"@id": "_:child", "name": "child"}]
    )
    options = {
        "version": 1.1,
        "context": {"p": "urn:p", "name": "urn:name"},
        "base": "https://example.org/requested/",
    }
    expected = ReferenceGraph().parse(
        data=document, format="json-ld", publicID="https://example.org/source/", **options
    )
    actual = Graph().parse(
        data=document, format="json-ld", publicID="https://example.org/source/", **options
    )
    assert isomorphic(actual, expected)
    assert list(actual.subjects(URIRef("urn:p"))) == [URIRef("https://example.org/requested/item")]
    child = actual.value(URIRef("https://example.org/requested/item"), URIRef("urn:p"))
    assert isinstance(child, BNode)
    assert (child, URIRef("urn:name"), Literal("child")) in actual


def test_explicit_base_uses_native_parser(monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError("native-supported options must not invoke a Python parser")

    monkeypatch.setattr(JsonLDParser, "parse", forbidden)
    actual = Graph().parse(
        data='{"@id":"item","urn:p":{"@id":"object"}}',
        format="json-ld",
        version=1.1,
        base="https://example.org/",
    )
    assert set(actual) == {
        (URIRef("https://example.org/item"), URIRef("urn:p"), URIRef("https://example.org/object"))
    }


def test_jsonld_options_do_not_allocate_a_reference_memory_store(monkeypatch):
    graph = Graph()

    def forbidden(*args, **kwargs):
        raise AssertionError("reference Memory store allocated")

    monkeypatch.setattr(Memory, "__init__", forbidden)
    graph.parse(
        data='{"@id":"urn:s","name":"value"}', format="json-ld", context={"name": "urn:name"}
    )
    assert (URIRef("urn:s"), URIRef("urn:name"), Literal("value")) in graph


def test_partial_plugin_failure_rolls_back_data_and_namespaces(monkeypatch):
    graph = Graph()
    graph.add((URIRef("urn:existing"), URIRef("urn:p"), Literal("old")))
    before = set(graph)
    namespaces = dict(graph.namespaces())

    def fail(self, source, sink, **options):
        sink.add((URIRef("urn:partial"), URIRef("urn:p"), Literal("new")))
        sink.bind("fresh", URIRef("urn:fresh:"))
        raise ValueError("late parser failure")

    monkeypatch.setattr(JsonLDParser, "parse", fail)
    with pytest.raises(ValueError, match="late parser failure"):
        graph.parse(data=DOCUMENT, format="json-ld", context={"p": "urn:p"})
    assert set(graph) == before
    assert dict(graph.namespaces()) == namespaces


def test_namespace_callback_failure_rolls_back_plugin_writes(monkeypatch):
    graph = Graph()
    before = dict(graph.namespaces())
    original_bind = RDFLibStore.bind

    def bind(store, prefix, namespace, override=True):
        if prefix == "fresh":
            raise ValueError("namespace callback failed")
        return original_bind(store, prefix, namespace, override)

    def parse(self, source, sink, **options):
        sink.add((URIRef("urn:partial"), URIRef("urn:p"), Literal("new")))
        sink.bind("fresh", URIRef("urn:fresh:"))

    monkeypatch.setattr(RDFLibStore, "bind", bind)
    monkeypatch.setattr(JsonLDParser, "parse", parse)
    with pytest.raises(ValueError, match="namespace callback failed"):
        graph.parse(data=DOCUMENT, format="json-ld", context={"p": "urn:p"})
    assert not graph
    assert dict(graph.namespaces()) == before


def test_selected_registered_parser_error_rolls_back(monkeypatch):
    graph = Graph()
    original = plugin.get

    class CustomParser:
        def parse(self, source, sink, **options):
            sink.add((URIRef("urn:partial"), URIRef("urn:p"), Literal("new")))
            raise ValueError("registered parser error")

    def get(name, kind):
        return CustomParser if name == "application/ld+json" else original(name, kind)

    monkeypatch.setattr(plugin, "get", get)
    with pytest.raises(ValueError, match="registered parser error"):
        graph.parse(data=DOCUMENT, format="application/ld+json", context={"p": "urn:p"})
    assert not graph


def test_named_jsonld_graph_uses_same_dataset_owner():
    document = '{"@id":"urn:named","@graph":[{"@id":"urn:s","p":"value"}]}'
    options = {"version": 1.1, "context": {"p": "urn:p"}}
    expected = ReferenceDataset()
    expected.parse(data=document, format="json-ld", **options)
    actual = StoreDataset()
    actual.parse(data=document, format="json-ld", **options)
    assert set(actual.quads()) == set(expected.quads())


def test_jsonld_skolemization_matches_reference():
    document = '{"@id":"_:child","urn:p":"value"}'
    expected = ReferenceGraph().parse(data=document, format="json-ld", skolemize=True)
    actual = Graph().parse(data=document, format="json-ld", skolemize=True)
    assert set(actual) == set(expected)
    assert all(isinstance(subject, URIRef) for subject in actual.subjects())


def test_jsonld_utf16_decode_failure_matches_reference_without_writes():
    document = DOCUMENT.encode("utf-16")
    with pytest.raises(UnicodeDecodeError):
        ReferenceGraph().parse(data=document, format="json-ld", encoding="utf-16")
    actual = Graph()
    with pytest.raises(UnicodeDecodeError):
        actual.parse(data=document, format="json-ld", encoding="utf-16")
    assert not actual


@pytest.mark.parametrize("options", [{"unknown_option": True}, {"generalized_rdf": True}])
def test_unsupported_options_never_mutate_graph(options):
    graph = Graph()
    before = dict(graph.namespaces())
    with pytest.raises((TypeError, NotImplementedError)):
        graph.parse(data=DOCUMENT, format="json-ld", **options)
    assert not graph
    assert dict(graph.namespaces()) == before


def test_caught_parser_option_failure_dooms_explicit_transaction():
    graph = Graph()
    with pytest.raises(RuntimeError):
        with graph.transaction() as transaction:
            transaction.add((URIRef("urn:pending"), URIRef("urn:p"), Literal("pending")))
            with pytest.raises(TypeError):
                transaction.parse(data=DOCUMENT, format="json-ld", unknown_option=True)
    assert not graph


def test_unsupported_jsonld_version_is_explicit_and_never_writes():
    graph = Graph()
    before = dict(graph.namespaces())
    with pytest.raises(NotImplementedError, match="Unsupported JSON-LD version"):
        graph.parse(data=DOCUMENT, format="json-ld", version=2.0)
    assert not graph
    assert dict(graph.namespaces()) == before


@pytest.mark.parametrize("rdf_format", ["json-ld", "application/ld+json"])
def test_default_jsonld_parse_still_uses_native_parser(monkeypatch, rdf_format):
    def forbidden(*args, **kwargs):
        raise AssertionError("default parser dispatched to Python")

    monkeypatch.setattr(JsonLDParser, "parse", forbidden)
    assert len(Graph().parse(data=DOCUMENT, format=rdf_format)) == 1


@pytest.mark.parametrize("options", [{}, {"version": 1.1}, {"base": "https://example.org/"}])
@pytest.mark.parametrize("dynamic", [False, True])
@pytest.mark.parametrize("content_type", ["application/json", "application/ld+json"])
def test_context_link_metadata_matches_reference_without_duplicate_callbacks(
    monkeypatch, options, dynamic, content_type
):
    class Source(StringInputSource):
        def __init__(self):
            super().__init__('{"@id":"urn:s","p":"value"}')
            self.content_type = content_type
            self.reads = {"url": 0, "links": 0}

        def metadata(self, name):
            self.reads[name] += 1
            return {
                "url": "https://example.org/document.json",
                "links": ['<context.json>; rel="http://www.w3.org/ns/json-ld#context"'],
            }[name]

    class LinkedSource(Source):
        @property
        def url(self):
            return self.metadata("url")

        @property
        def links(self):
            return self.metadata("links")

    class DynamicSource(Source):
        def __getattr__(self, name):
            if name in {"url", "links"}:
                return self.metadata(name)
            raise AttributeError(name)

    def load(url, *args, **kwargs):
        assert url == "https://example.org/context.json"
        return {"@context": {"p": "urn:p"}}, None

    monkeypatch.setattr("rdflib.plugins.shared.jsonld.context.source_to_json", load)
    source_type = DynamicSource if dynamic else LinkedSource
    reference_source, native_source = source_type(), source_type()
    expected = ReferenceGraph().parse(source=reference_source, format="json-ld", **options)
    actual = Graph().parse(source=native_source, format="json-ld", **options)
    triples = {(URIRef("urn:s"), URIRef("urn:p"), Literal("value"))}
    assert (
        set(actual) == set(expected) == (triples if content_type == "application/json" else set())
    )
    assert native_source.reads == reference_source.reads


@pytest.mark.parametrize("options", [{}, {"version": 1.1}, {"base": "https://example.org/"}])
def test_native_jsonld_preserves_explicit_blank_ids(options):
    document = '{"@id":"_:shared","urn:new":"new"}'
    expected, actual = ReferenceGraph(), Graph()
    for graph in (expected, actual):
        graph.add((BNode("shared"), URIRef("urn:old"), Literal("old")))
        graph.parse(data=document, format="json-ld", **options)
    assert set(actual) == set(expected)
    assert (BNode("shared"), URIRef("urn:new"), Literal("new")) in actual


@pytest.mark.parametrize(
    "rdf_format,document", [("json-ld", '{"urn:p":"value"}'), ("turtle", '[] <urn:p> "value" .')]
)
def test_generated_blank_nodes_remain_fresh_across_documents(rdf_format, document):
    expected, actual = ReferenceGraph(), Graph()
    for graph in (expected, actual):
        graph.parse(data=document, format=rdf_format)
        graph.parse(data=document, format=rdf_format)
    assert len(set(actual.subjects())) == len(set(expected.subjects())) == 2
    assert len(actual) == len(expected) == 2
    assert {(p, o) for _, p, o in actual} == {(p, o) for _, p, o in expected}


def test_turtle_explicit_blank_labels_keep_per_document_scope():
    expected, actual = ReferenceGraph(), Graph()
    for graph in (expected, actual):
        graph.add((BNode("shared"), URIRef("urn:old"), Literal("old")))
        graph.parse(data='_:shared <urn:new> "one" .', format="turtle")
        graph.parse(data='_:shared <urn:new> "two" .', format="turtle")
    assert len(set(actual.subjects())) == len(set(expected.subjects())) == 3
    assert len(actual) == len(expected) == 3
    assert {(p, o) for _, p, o in actual} == {(p, o) for _, p, o in expected}
