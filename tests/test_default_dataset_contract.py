"""The default Dataset export must support ordinary RDFLib Dataset usage."""

import pytest
from rdflib import BNode, Literal, Namespace, URIRef
from rdflib import Dataset as RDFLibDataset
from rdflib import Graph as RDFLibGraph
from rdflib.namespace import XSD
from rdflib_turbo import Dataset, StoreDataset

EX = Namespace("urn:default-dataset:")


def term_key(value):
    return (
        type(value).__name__,
        str(value),
        str(value.datatype) if isinstance(value, Literal) and value.datatype else None,
        value.language if isinstance(value, Literal) else None,
    )


def quad_keys(dataset):
    return {tuple(term_key(term) for term in quad) for quad in dataset.quads((None,) * 4)}


def populate(dataset):
    dataset.bind("ex", EX)
    dataset.default_graph.add((EX.s, EX.p, Literal("plain")))
    dataset.graph(EX.named).add((EX.s, EX.p, Literal("plain")))
    dataset.graph(BNode("named-blank")).add(
        (EX.b, EX.p, Literal("+001", datatype=XSD.integer, normalize=False))
    )
    dataset.graph(EX.empty)
    return dataset


@pytest.mark.parametrize("factory", (Dataset, StoreDataset))
def test_default_dataset_constructor_accepts_normal_options(factory):
    expected = RDFLibDataset(default_union=True, default_graph_base="urn:default-base:")
    actual = factory(default_union=True, default_graph_base="urn:default-base:")
    assert isinstance(actual, RDFLibDataset)
    assert isinstance(actual, RDFLibGraph)
    assert actual.default_union is expected.default_union
    assert actual.default_graph.base == expected.default_graph.base
    assert actual.default_graph.identifier == expected.default_graph.identifier


@pytest.mark.parametrize("factory", (Dataset, StoreDataset))
def test_default_dataset_views_quads_namespaces_and_duplicate_lengths(factory):
    expected, actual = populate(RDFLibDataset()), populate(factory())
    assert isinstance(actual, RDFLibDataset)
    assert quad_keys(actual) == quad_keys(expected)
    assert len(actual) == len(expected)
    assert {term_key(g.identifier) for g in actual.graphs()} == {
        term_key(g.identifier) for g in expected.graphs()
    }
    assert dict(actual.namespaces()) == dict(expected.namespaces())
    assert all(isinstance(namespace, URIRef) for _, namespace in actual.namespaces())
    assert isinstance(actual.graph(EX.named), RDFLibGraph)
    actual.graph(EX.named).remove((EX.s, EX.p, None))
    expected.graph(EX.named).remove((EX.s, EX.p, None))
    assert quad_keys(actual) == quad_keys(expected)
    assert (EX.s, EX.p, Literal("plain")) in actual.default_graph


@pytest.mark.parametrize("factory", (Dataset, StoreDataset))
def test_default_dataset_parse_and_query_standard_bindings(factory):
    expected, actual = RDFLibDataset(), factory()
    document = '@prefix ex: <urn:default-dataset:> . ex:s ex:p "default" . ex:named { ex:n ex:p "named" . }'
    for dataset in (expected, actual):
        assert dataset.parse(data=document, format="trig") is dataset
    assert quad_keys(actual) == quad_keys(expected)
    query = "SELECT ?s ?o WHERE { ?s ?p ?o } ORDER BY ?s ?o"
    assert list(actual.query(query, initBindings={"p": EX.p})) == list(
        expected.query(query, initBindings={"p": EX.p})
    )
    query = "SELECT ?o WHERE { <urn:default-dataset:s> ex:p ?o }"
    assert list(actual.query(query, initNs={"ex": EX})) == list(
        expected.query(query, initNs={"ex": EX})
    )
    query = "SELECT ?g ?o WHERE { GRAPH ?g { ?s ex:p ?o }} ORDER BY ?g ?o"
    assert list(actual.query(query, initNs={"ex": EX})) == list(
        expected.query(query, initNs={"ex": EX})
    )


@pytest.mark.parametrize("factory", (Dataset, StoreDataset))
def test_default_dataset_default_context_alias_retains_reference_warning(factory):
    actual = factory()
    with pytest.deprecated_call(match="Dataset.default_context is deprecated"):
        context = actual.default_context
    assert context is actual.default_graph


@pytest.mark.parametrize("factory", (Dataset, StoreDataset))
def test_default_dataset_copy_owns_independent_rdf_and_namespace_state(factory):
    source = populate(factory())
    copied = source.copy()
    assert type(copied) is type(source)
    assert copied.store is not source.store
    assert quad_keys(copied) == quad_keys(source)
    assert {g.identifier for g in copied.graphs()} == {g.identifier for g in source.graphs()}
    copied.bind("private-copy", "urn:copy-only:")
    copied.default_graph.add((EX.changed, EX.p, Literal("copy")))
    assert "private-copy" not in dict(source.namespaces())
    assert (EX.changed, EX.p, None) not in source.default_graph


@pytest.mark.parametrize("factory", (Dataset, StoreDataset))
def test_default_dataset_transaction_commit_and_abort_keep_one_owner(factory):
    source = populate(factory())
    before = quad_keys(source)
    namespaces = dict(source.namespaces())
    with pytest.raises(RuntimeError, match="abort fixture"):
        with source.transaction() as pending:
            assert type(pending) is type(source)
            pending.default_graph.add((EX.changed, EX.p, Literal("abort")))
            pending.graph(EX.empty_pending)
            pending.bind("pending-only", "urn:pending:")
            assert quad_keys(source) == before
            raise RuntimeError("abort fixture")
    assert quad_keys(source) == before
    assert dict(source.namespaces()) == namespaces
    assert EX.empty_pending not in {g.identifier for g in source.graphs()}
    with source.transaction() as pending:
        pending.default_graph.add((EX.changed, EX.p, Literal("commit")))
        pending.bind("committed", "urn:committed:")
    assert (EX.changed, EX.p, Literal("commit")) in source.default_graph
    assert dict(source.namespaces())["committed"] == URIRef("urn:committed:")
