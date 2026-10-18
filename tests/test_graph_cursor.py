"""Scoped Graph reads cross a typed, bounded native cursor boundary."""

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib_turbo import Graph, StoreDataset
from rdflib_turbo.graph import RDFLibStore


def forbidden(*args, **kwargs):
    raise AssertionError("eager JSON/triple transport executed")


def term_factories(literal_restore=None):
    from rdflib_turbo._literal import restore
    from rdflib_turbo._native import NativeTermFactories

    return NativeTermFactories(
        URIRef, BNode, restore if literal_restore is None else literal_restore, str.__new__
    )


def test_scoped_triples_do_not_use_json_and_reuse_bound_terms(monkeypatch):
    graph = Graph()
    subject, predicate = URIRef("urn:s"), URIRef("urn:p")
    value = Literal(
        "01", datatype=URIRef("http://www.w3.org/2001/XMLSchema#integer"), normalize=False
    )
    graph.add((subject, predicate, value))
    import rdflib_turbo.graph as facade

    monkeypatch.setattr(facade.json, "loads", forbidden)
    monkeypatch.setattr(facade.json, "dumps", forbidden)
    triple = next(graph.triples((subject, predicate, None)))
    assert triple[0] is subject
    assert triple[1] is predicate
    assert triple[2] == value
    assert str(triple[2]) == "01"


def test_unbound_triples_forward_the_original_native_row_tuple(monkeypatch):
    graph = Graph().add((URIRef("urn:s"), URIRef("urn:p"), Literal("value")))
    native = graph.store.backend._native
    captured = []

    class CaptureBatch:
        def read_pattern(self, *args):
            batch, cursor = native.read_pattern(*args)
            captured.append(batch[0])
            return batch, cursor

    monkeypatch.setattr(graph.store.backend, "_native", CaptureBatch())
    rows = graph.triples((None, None, None))
    assert next(rows) is captured[0]
    rows.close()


@pytest.mark.parametrize("bound_mask", range(8))
def test_all_bound_pattern_combinations_preserve_exact_terms_and_caller_identity(bound_mask):
    datatype = URIRef("http://www.w3.org/2001/XMLSchema#integer")
    triple = (URIRef("urn:s"), URIRef("urn:p"), Literal("01", datatype=datatype, normalize=False))
    graph = Graph().add(triple)
    supplied = (URIRef("urn:s"), URIRef("urn:p"), Literal("01", datatype=datatype, normalize=False))
    pattern = tuple(
        term if bound_mask & (1 << index) else None for index, term in enumerate(supplied)
    )
    actual = list(graph.triples(pattern))
    assert actual == [triple]
    for index, term in enumerate(pattern):
        if term is not None:
            assert actual[0][index] is term
    assert str(actual[0][2]) == "01"
    assert actual[0][2].datatype == datatype


@pytest.mark.parametrize(
    "method,args,expected",
    [
        ("subjects", (), [URIRef("urn:s")]),
        ("predicates", (), [URIRef("urn:p")]),
        ("objects", (), [Literal("value")]),
        ("predicate_objects", (URIRef("urn:s"),), [(URIRef("urn:p"), Literal("value"))]),
        ("subject_objects", (URIRef("urn:p"),), [(URIRef("urn:s"), Literal("value"))]),
        ("subject_predicates", (Literal("value"),), [(URIRef("urn:s"), URIRef("urn:p"))]),
    ],
)
def test_projection_skips_full_triple_transport(monkeypatch, method, args, expected):
    graph = Graph().add((URIRef("urn:s"), URIRef("urn:p"), Literal("value")))
    monkeypatch.setattr(RDFLibStore, "triples", forbidden)
    assert list(getattr(graph, method)(*args)) == expected


def test_cursor_batches_are_capped_and_can_be_closed_early():
    graph = Graph().parse(
        data="\n".join(f'<urn:s{i}> <urn:p> "v" .' for i in range(400)), format="nt"
    )
    batch, cursor = graph.store.backend._native.read_pattern(
        (None, None, None), None, 1, term_factories()
    )
    assert cursor is not None
    assert 0 < len(batch) <= 128
    assert all(type(row[0]) is URIRef and row[1:] == (None, None) for row in batch)
    second, finished = cursor.next_batch()
    assert 0 < len(second) <= 128
    assert finished is False
    cursor.close()
    assert cursor.next_batch() == ([], True)
    assert len(graph) == 400


def test_small_scan_returns_first_rows_without_allocating_python_cursor():
    graph = Graph().add((BNode("s"), URIRef("urn:p"), Literal("value", lang="en")))
    batch, cursor = graph.store.backend._native.read_pattern(
        (None, None, None), None, 7, term_factories()
    )
    assert cursor is None
    assert batch == [(BNode("s"), URIRef("urn:p"), Literal("value", lang="en"))]
    assert tuple(type(value) for value in batch[0]) == (BNode, URIRef, Literal)
    assert batch[0][2].value == "value"
    assert batch[0][2].language == "en"
    assert graph.store.backend._native.read_pattern(
        (None, None, None), None, 0, term_factories()
    ) == (
        [(None, None, None)],
        None,
    )


@pytest.mark.parametrize("row_count", [1, 400])
def test_first_batch_factory_failure_publishes_no_cursor_and_preserves_graph(row_count):
    graph = Graph().parse(
        data="\n".join(f'<urn:s{i}> <urn:p> "v" .' for i in range(row_count)), format="nt"
    )
    failure = ValueError("first literal restoration failed")
    calls = 0
    published = []

    def literal_restore(*args):
        nonlocal calls
        calls += 1
        raise failure

    with pytest.raises(ValueError) as captured:
        published.append(
            graph.store.backend._native.read_pattern(
                (None, None, None), None, 4, term_factories(literal_restore)
            )
        )
    assert captured.value is failure
    assert calls == 1
    assert published == []
    assert len(graph) == row_count
    assert sum(1 for _ in graph.objects()) == row_count
    triple = (URIRef("urn:after-failed-read"), URIRef("urn:p"), Literal("after"))
    graph.add(triple)
    assert list(graph.triples((triple[0], None, None))) == [triple]
    assert len(graph) == row_count + 1


def test_native_factory_failure_closes_continuation_and_preserves_exception():
    from rdflib_turbo._literal import restore

    graph = Graph().parse(
        data="\n".join(f'<urn:s{i}> <urn:p> "v" .' for i in range(400)), format="nt"
    )
    failure = ValueError("literal factory failed")
    failing = False

    def literal_restore(*args):
        if failing:
            raise failure
        return restore(*args)

    batch, cursor = graph.store.backend._native.read_pattern(
        (None, None, None), None, 4, term_factories(literal_restore)
    )
    assert len(batch) == 128 and cursor is not None
    failing = True
    with pytest.raises(ValueError) as captured:
        cursor.next_batch()
    assert captured.value is failure
    assert cursor.next_batch() == ([], True)


@pytest.mark.parametrize(
    "factories,error",
    [
        (None, TypeError),
        ((), TypeError),
        ((42, BNode, str, str.__new__), TypeError),
        ((int, BNode, str, str.__new__), TypeError),
        ((URIRef, BNode, 42, str.__new__), TypeError),
        ((URIRef, BNode, str, 42), TypeError),
    ],
)
def test_native_rejects_invalid_term_factories_before_scanning(factories, error):
    graph = Graph()
    with pytest.raises(error):
        graph.store.backend._native.read_pattern((None, None, None), None, 7, factories)


def test_native_factories_can_reenter_and_close_the_continuation():
    from rdflib_turbo._literal import restore

    graph = Graph().parse(
        data="\n".join(f'<urn:s{i}> <urn:p> "v" .' for i in range(400)), format="nt"
    )
    holder = []

    def literal_restore(*args):
        if holder:
            holder.pop().close()
        return restore(*args)

    _, cursor = graph.store.backend._native.read_pattern(
        (None, None, None), None, 4, term_factories(literal_restore)
    )
    holder.append(cursor)
    second, _ = cursor.next_batch()
    assert len(second) == 128
    assert cursor.next_batch() == ([], True)


def test_native_factory_reference_cycles_release_the_cursor():
    import gc
    import weakref

    from rdflib_turbo._literal import restore

    graph = Graph().parse(
        data="\n".join(f'<urn:s{i}> <urn:p> "v" .' for i in range(400)), format="nt"
    )

    def make_cycle():
        holder = []

        def literal_restore(*args):
            assert len(holder) <= 1
            return restore(*args)

        _, cursor = graph.store.backend._native.read_pattern(
            (None, None, None), None, 4, term_factories(literal_restore)
        )
        holder.append(cursor)
        return weakref.ref(literal_restore)

    factory = make_cycle()
    gc.collect()
    assert factory() is None


def test_scoped_cursor_preserves_named_and_default_graphs():
    dataset = StoreDataset()
    triple = (BNode("s"), URIRef("urn:p"), Literal("v", lang="en"))
    dataset.default_graph.add(triple)
    named = dataset.graph(URIRef("urn:named"))
    named.add((triple[0], triple[1], Literal("named")))
    assert list(dataset.default_graph) == [triple]
    assert list(named.objects()) == [Literal("named")]
    assert list(dataset.default_graph.predicate_objects(triple[0])) == [(triple[1], triple[2])]


def test_projection_lists_paths_unique_and_nested_transaction_reads():
    from rdflib import Graph as ReferenceGraph

    actual, expected = Graph(), ReferenceGraph()
    s, p, q, a, b = map(URIRef, ("urn:s", "urn:p", "urn:q", "urn:a", "urn:b"))
    triples = [(s, p, a), (s, q, a), (a, p, b)]
    for graph in (actual, expected):
        for triple in triples:
            graph.add(triple)
    for method in (
        "subjects",
        "predicates",
        "objects",
        "subject_objects",
        "predicate_objects",
        "subject_predicates",
    ):
        assert set(getattr(actual, method)(unique=True)) == set(
            getattr(expected, method)(unique=True)
        )
    assert set(actual.subjects(p, [a, b])) == {s, a}
    assert set(actual.objects([s, a], p)) == {a, b}
    path = p / p
    assert list(actual.subjects(path, b)) == [s]
    assert list(actual.objects(s, path)) == [b]
    assert list(actual.subject_objects(path)) == [(s, b)]
    with actual.transaction() as transaction:
        for method in (
            "subjects",
            "predicates",
            "objects",
            "subject_objects",
            "predicate_objects",
            "subject_predicates",
        ):
            assert set(getattr(transaction, method)()) == set(getattr(expected, method)())
        assert {
            subject: set(transaction.predicate_objects(subject))
            for subject in transaction.subjects()
        } == {
            s: {(p, a), (q, a)},
            a: {(p, b)},
        }


def test_projection_respects_graph_subclass_triples_override():
    class FilteredGraph(Graph):
        def triples(self, pattern):
            for triple in super().triples(pattern):
                if triple[0] == URIRef("urn:visible"):
                    yield triple

    graph = FilteredGraph()
    for subject in ("urn:visible", "urn:hidden"):
        graph.add((URIRef(subject), URIRef("urn:p"), Literal(subject)))
    assert list(graph.subjects()) == [URIRef("urn:visible")]
    assert list(graph.objects()) == [Literal("urn:visible")]


def test_typed_boundary_rejects_unknown_terms():
    graph = Graph()
    with pytest.raises(TypeError, match="Unsupported RDF term"):
        list(graph.triples((42, None, None)))


def test_graph_iteration_consumes_every_bounded_batch_and_can_stop_early():
    # bounded: 400 fixture triples exceed the cursor's 128-row batch cap.
    graph = Graph().parse(
        data="\n".join(f'<urn:s{i}> <urn:p> "v" .' for i in range(400)), format="nt"
    )
    assert len(set(graph)) == 400
    assert len(set(graph.subjects())) == 400
    rows = graph.subjects()
    assert isinstance(next(rows), URIRef)
    rows.close()
    assert len(graph) == 400
