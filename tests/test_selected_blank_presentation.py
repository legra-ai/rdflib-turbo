"""Selected source descriptions include the actual blank-node closure."""

import pyshacl.rdfutil.stringify as reference
import pytest
from rdflib import BNode, Dataset, Graph, Literal, Namespace, URIRef
from rdflib.collection import Collection
from rdflib.namespace import OWL, RDF
from rdflib_turbo import _literal

EX = Namespace("urn:source-description:")


def fixture(kind):
    graph = Dataset() if kind == "dataset" else Graph()
    graph.bind("ex", EX)
    target = graph.graph(EX.context) if kind == "dataset" else graph
    # The reference's process cache uses graph id and blank-node label. Fresh
    # labels keep independently allocated fixtures from sharing stale entries.
    root = BNode()
    if kind == "list":
        Collection(target, root, [Literal("z"), Literal("a"), EX.value])
    elif kind == "cycle":
        target.add((root, EX.next, root))
    elif kind == "same-as":
        target.add((root, OWL.sameAs, root))
    elif kind == "depth":
        parent = root
        for index in range(8):
            child = BNode(f"child-{index}")
            target.add((parent, EX.next, child))
            parent = child
    elif kind != "empty":
        target.add((root, RDF.type, EX.Class))
        target.add((root, EX.label, Literal('quoted "value"\nline')))
        target.add((root, EX.label, Literal("another")))
    return graph, root


@pytest.mark.parametrize(
    "kind", ["empty", "described", "list", "cycle", "same-as", "depth", "dataset"]
)
@pytest.mark.parametrize("name", ["_selected_stringify", "_selected_presentation"])
def test_selected_blank_description_matches_reference(kind, name):
    graph, root = fixture(kind)
    expected = reference.stringify_node(graph, root)
    assert getattr(_literal, name)(graph, root) == expected


def test_repeated_child_uses_one_render_local_description():
    graph, root = Graph(), BNode()
    child = BNode()
    graph.add((root, EX.a, child))
    graph.add((root, EX.b, child))
    graph.add((child, EX.name, Literal("shared")))
    assert _literal._selected_stringify(graph, root) == reference.stringify_node(graph, root)


def test_source_predicate_callbacks_preserve_empty_and_nonterm_predicates():
    class Source(Graph):
        def predicates(self, subject=None, object=None, unique=False):
            return iter(("urn:empty", "urn:actual"))

        def objects(self, subject=None, predicate=None, unique=False):
            return iter(()) if predicate == "urn:empty" else iter((EX.value,))

    graph, root = Source(), BNode()
    assert _literal._selected_stringify(graph, root) == reference.stringify_node(graph, root)


@pytest.mark.parametrize("found", [False, True])
def test_dataset_context_search_preserves_object_search_and_graph_contexts(found):
    context, root = Graph(), BNode()
    context.add((root, EX.name, Literal("found")))

    class Source(Dataset):
        def quads(self, quad=None):
            if quad[0] is not None:
                return iter(((root, EX.name, Literal("ignored"), None),))
            return iter(((EX.other, EX.link, root, context),)) if found else iter(())

    graph = Source()
    if found:
        assert _literal._selected_stringify(graph, root) == reference.stringify_node(graph, root)
    else:
        with pytest.raises(LookupError, match="Cannot find node"):
            _literal._selected_stringify(graph, root)
        assert _literal._selected_presentation(graph, root) == str(root)


@pytest.mark.parametrize("kind", ["language", "unknown", "object", "datatype-string", "plain"])
def test_selected_literal_presentation_uses_actual_cache_and_datatype(kind):
    from rdflib.namespace import XSD

    graph = Graph()
    node = Literal("lexical", lang="en") if kind == "language" else Literal("lexical")
    if kind == "unknown":
        node = Literal("lexical", datatype=EX.unknown)
    elif kind == "object":
        node._value = object()
    elif kind == "datatype-string":
        node._datatype = str(XSD.string)
    assert _literal._selected_presentation(graph, node) == reference.stringify_node(graph, node)


def test_selected_formatter_accepts_graph_namespace_manager_and_nonterm_values():
    graph = Graph()
    assert _literal._selected_stringify(graph, 17, graph) == reference.stringify_node(
        graph, 17, graph
    )


def test_selected_uri_n3_error_uses_actual_uri_text():
    class BrokenURI(URIRef):
        def n3(self, namespace_manager=None):
            raise RuntimeError("source n3 error")

    node, graph = BrokenURI("urn:actual"), Graph()
    assert _literal._selected_presentation(graph, node) == reference.stringify_node(graph, node)


def test_selected_formatter_fails_without_namespace_authority():
    class Source(Graph):
        @property
        def namespace_manager(self):
            return None

    for formatter in (_literal._selected_stringify, reference.stringify_node):
        with pytest.raises(RuntimeError, match="no namespaces"):
            formatter(Source(), EX.value)


def test_current_scalar_report_transport_rejects_surrogates_without_lossy_repair():
    with pytest.raises(UnicodeError, match="lossless non-scalar"):
        _literal._selected_presentation(Graph(), Literal("\ud800"))
