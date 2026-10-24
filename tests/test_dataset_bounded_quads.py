"""Dataset quad iteration reads physical rows through the owning native cursor."""

from itertools import product

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib import Dataset as ReferenceDataset
from rdflib.namespace import XSD
from rdflib_turbo import StoreDataset
from rdflib_turbo import _reads as reads
from rdflib_turbo import graph as facade
from rdflib_turbo._literal import restore
from rdflib_turbo._native import NativeTermFactories


def populate(dataset):
    graphs = (
        dataset.default_graph,
        dataset.graph(URIRef("urn:graph")),
        dataset.graph(BNode("urn:graph")),
    )
    for index in range(450):
        triple = (Literal(f"subject-{index}"), BNode("predicate"), Literal(str(index)))
        graphs[index % 3].add(triple)
    triple = (URIRef("urn:shared"), URIRef("urn:p"), Literal("same"))
    for graph in graphs:
        graph.add(triple)
    for obj in (
        Literal("+001"),
        Literal("+001", datatype=XSD.integer, normalize=False),
        Literal("+001", datatype=URIRef(""), normalize=False),
    ):
        graphs[1].add((URIRef("urn:typed"), URIRef("urn:p"), obj))
    dataset.graph(URIRef("urn:empty"))
    return dataset


def keys(rows):
    return {
        tuple(
            (
                type(term).__name__,
                str(term),
                str(term.datatype)
                if isinstance(term, Literal) and term.datatype is not None
                else None,
                term.language if isinstance(term, Literal) else None,
            )
            for term in row
        )
        for row in rows
    }


def forbid_json(*args, **kwargs):
    pytest.fail("Dataset read hydrated a JSON document")


def test_physical_quad_masks_preserve_default_and_named_graph_kinds_without_json(monkeypatch):
    expected = populate(ReferenceDataset())
    actual = populate(StoreDataset())
    probes = (
        (None, None, None, None),
        (URIRef("urn:shared"), URIRef("urn:p"), Literal("same"), URIRef("urn:graph")),
        (
            URIRef("urn:typed"),
            URIRef("urn:p"),
            Literal("+001", datatype=XSD.integer, normalize=False),
            URIRef("urn:graph"),
        ),
        (
            URIRef("urn:typed"),
            URIRef("urn:p"),
            Literal("+001", datatype=URIRef(""), normalize=False),
            URIRef("urn:graph"),
        ),
        (Literal("subject-2"), BNode("predicate"), Literal("2"), BNode("urn:graph")),
        (Literal("subject-0"), BNode("predicate"), Literal("0"), actual.default_graph.identifier),
    )
    for probe in probes:
        for bound in product((False, True), repeat=4):
            pattern = tuple(term if selected else None for term, selected in zip(probe, bound))
            reference = keys(expected.quads(pattern))
            with monkeypatch.context() as patch:
                patch.setattr(facade.json, "loads", forbid_json)
                assert keys(actual.quads(pattern)) == reference


def test_physical_quad_cursor_outlives_closed_store_without_materializing_json(monkeypatch):
    expected = populate(ReferenceDataset())
    actual = populate(StoreDataset())
    with monkeypatch.context() as patch:
        patch.setattr(facade.json, "loads", forbid_json)
        rows = actual.quads((None,) * 4)
        first = next(rows)
        actual.close()
        assert keys((first, *rows)) == keys(expected.quads((None,) * 4))


def test_pending_quad_scope_selects_statements_then_all_their_contexts():
    expected = populate(ReferenceDataset())
    actual = StoreDataset()
    with actual.transaction() as pending:
        populate(pending)
        for graph in (
            pending.default_graph.identifier,
            URIRef("urn:graph"),
            BNode("urn:graph"),
            URIRef("urn:absent"),
        ):
            probe = (URIRef("urn:shared"), URIRef("urn:p"), Literal("same"), graph)
            for bound in product((False, True), repeat=4):
                pattern = tuple(term if selected else None for term, selected in zip(probe, bound))
                assert keys(pending.quads(pattern)) == keys(expected.quads(pattern))
            selected = pending.graph(graph)
            reference = expected.graph(graph)
            assert contextual_rows(pending.store.triples((None,) * 3, selected)) == contextual_rows(
                expected.store.triples((None,) * 3, reference)
            )


def contextual_rows(rows):
    return {
        (
            next(iter(keys((triple,)))),
            frozenset(keys((context.identifier,) for context in contexts)),
        )
        for triple, contexts in rows
    }


def test_union_rows_and_lazy_contexts_keep_the_same_snapshot_after_store_close():
    expected = populate(ReferenceDataset())
    actual = populate(StoreDataset())
    reference = contextual_rows(expected.store.triples((None,) * 3))
    rows = actual.store.triples((None,) * 3)
    first = next(rows)
    actual.close()
    # Later rows and context iterators, not just the first batch, retain the read.
    assert contextual_rows((first, *rows)) == reference


@pytest.mark.parametrize("scoped", (False, True))
def test_lazy_contexts_exclude_writes_after_the_union_snapshot_was_selected(scoped):
    actual = populate(StoreDataset())
    context = actual.graph(URIRef("urn:graph")) if scoped else None
    rows = actual.store.triples((URIRef("urn:shared"), URIRef("urn:p"), Literal("same")), context)
    triple, contexts = next(rows)
    expected = {
        actual.default_graph.identifier,
        URIRef("urn:graph"),
        BNode("urn:graph"),
    }
    actual.graph(URIRef("urn:late")).add(triple)
    assert {context.identifier for context in contexts} == expected


def test_union_does_not_restore_context_identifiers_until_requested(monkeypatch):
    actual = populate(StoreDataset())
    failure = RuntimeError("context identifier restoration was requested")
    original_term = facade._term

    def allocate(cls, text):
        if text == "urn:graph":
            raise failure
        return str.__new__(cls, text)

    def convert(term):
        if term is not None and term["value"] == "urn:graph":
            raise failure
        return original_term(term)

    monkeypatch.setattr(
        reads, "_TERM_FACTORIES", NativeTermFactories(URIRef, BNode, restore, allocate, True)
    )
    monkeypatch.setattr(facade, "_term", convert)
    rows = actual.store.triples((URIRef("urn:shared"), URIRef("urn:p"), Literal("same")))
    triple, contexts = next(rows)
    assert triple == (URIRef("urn:shared"), URIRef("urn:p"), Literal("same"))
    with pytest.raises(RuntimeError) as caught:
        list(contexts)
    assert caught.value is failure
