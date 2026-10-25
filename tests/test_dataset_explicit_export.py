"""Explicit RDFLib export streams full terms without losing Dataset metadata."""

from itertools import product

from rdflib import BNode, Literal, Namespace, URIRef
from rdflib import Dataset as RDFLibDataset
from rdflib.namespace import XSD
from rdflib_turbo import StoreDataset

EX = Namespace("urn:dataset-export:")


def term_key(term):
    return (
        type(term).__name__,
        str(term),
        str(term.datatype) if isinstance(term, Literal) and term.datatype else None,
        term.language if isinstance(term, Literal) else None,
    )


def observations(dataset):
    return {
        "quads": {tuple(term_key(term) for term in row) for row in dataset.quads((None,) * 4)},
        "graphs": {term_key(graph.identifier) for graph in dataset.graphs()},
        "namespaces": dict(dataset.namespaces()),
        "default_union": dataset.default_union,
        "default_base": dataset.default_graph.base,
    }


def test_explicit_dataset_export_preserves_full_terms_and_empty_domains(monkeypatch):
    expected = RDFLibDataset(default_union=True, default_graph_base="urn:export-base:")
    source = StoreDataset(default_union=True, default_graph_base="urn:export-base:")
    subjects = (EX.subject, BNode("shared"), Literal("literal subject", lang="en"))
    predicates = (
        EX.predicate,
        BNode("predicate"),
        Literal("+001", datatype=XSD.integer, normalize=False),
    )
    for dataset in (expected, source):
        dataset.bind("ex", EX)
        dataset.bind("rdf", "urn:explicit-rdf:", replace=True)
        dataset.graph(EX.empty)
        dataset.graph(BNode("empty-blank"))
        for index, (subject, predicate) in enumerate(product(subjects, predicates)):
            graph = dataset.default_graph if index % 2 else dataset.graph(BNode("named-blank"))
            graph.add((subject, predicate, Literal("plain")))
            graph.add((subject, predicate, Literal("plain", datatype=XSD.string)))
    expected_observation = observations(expected)
    assert observations(source) == expected_observation

    def forbid_transport(*args, **kwargs):
        raise AssertionError("Export must stream scoped native rows, not serialize or group quads")

    monkeypatch.setattr(source, "serialize", forbid_transport)
    monkeypatch.setattr(source, "quads", forbid_transport)
    monkeypatch.setattr(source.store, "triples", forbid_transport)
    exported = source.to_rdflib()
    assert type(exported) is RDFLibDataset
    assert observations(exported) == expected_observation
    assert all(isinstance(namespace, URIRef) for _, namespace in exported.namespaces())
    exported.default_graph.add((EX.changed, EX.predicate, Literal("independent")))
    exported.bind("export-only", "urn:export-only:")
    assert (EX.changed, EX.predicate, None) not in source.default_graph
    assert "export-only" not in dict(source.namespaces())
