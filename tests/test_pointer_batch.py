"""Bounded identifier reuse preserves kinds, callbacks and cursor cleanup."""

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib_turbo import Graph
from rdflib_turbo._literal import restore
from rdflib_turbo._native import NativeTermFactories

PREDICATE = URIRef("urn:shared:predicate:long-enough-for-dictionary")


def scan(graph, factories, mask=2):
    rows, cursor = graph.store.backend._native.read_pattern(
        (None, None, None), None, mask, factories
    )
    batches = [rows]
    if cursor is not None:
        while True:
            rows, finished = cursor.next_batch()
            batches.append(rows)
            if finished:
                break
    return batches


def graph_rows(count=300, predicate=PREDICATE):
    graph = Graph()
    for index in range(count):
        graph.add((URIRef(f"urn:subject:{index}"), predicate, Literal(index)))
    return graph


def test_identifier_reuse_is_explicit_and_scoped_to_one_batch():
    graph = graph_rows()
    calls = []

    def allocate(cls, value):
        calls.append(value)
        return str.__new__(cls, value)

    default = NativeTermFactories(URIRef, BNode, restore, allocate)
    assert sum(map(len, scan(graph, default))) == 300
    assert len(calls) == 300
    calls.clear()
    enabled = NativeTermFactories(URIRef, BNode, restore, allocate, True)
    batches = scan(graph, enabled)
    assert sum(map(len, batches)) == 300
    assert len(calls) == len([batch for batch in batches if batch])
    for batch in batches:
        assert all(row[1] is batch[0][1] for row in batch)
        assert all(type(row[1]) is URIRef and row[1] == PREDICATE for row in batch)
    assert batches[0][0][1] is not batches[1][0][1]


@pytest.mark.parametrize(("characters", "expected_calls"), [(126, 1), (127, 20)])
def test_identifier_byte_boundary_and_literal_callbacks_never_reused(characters, expected_calls):
    calls = []
    literals = []

    def allocate(cls, value):
        calls.append(value)
        return str.__new__(cls, value)

    def literal(*args):
        literals.append(args)
        return restore(*args)

    graph = graph_rows(20, URIRef("urn:" + "é" * characters))
    factories = NativeTermFactories(URIRef, BNode, literal, allocate, True)
    batches = scan(graph, factories, 6)
    assert len(calls) == expected_calls
    assert len(literals) == 20
    assert sum(map(len, batches)) == 20


def test_reuse_preserves_identifier_kind_even_for_equal_lexical_values():
    text = "urn:identifier:shared-lexical-value"
    graph = Graph()
    for index in range(12):
        kind = URIRef if index % 2 else BNode
        graph.add((URIRef(f"urn:kind:subject:{index}"), PREDICATE, kind(text)))
    factories = NativeTermFactories(URIRef, BNode, restore, str.__new__, True)
    batches = scan(graph, factories, 4)
    assert [len(batch) for batch in batches] == [12]
    values = [row[2] for row in batches[0]]
    assert {type(value) for value in values} == {URIRef, BNode}
    assert all(str(value) == text for value in values)
    for kind in (URIRef, BNode):
        assert len({id(value) for value in values if type(value) is kind}) == 1


def test_factory_reentry_and_original_exception_are_preserved():
    graph = graph_rows(20)
    failure = ValueError("private callback failed")
    seen = []

    def allocate(cls, value):
        seen.append(len(graph))
        raise failure

    factories = NativeTermFactories(URIRef, BNode, restore, allocate, True)
    with pytest.raises(ValueError) as caught:
        scan(graph, factories)
    assert caught.value is failure and seen == [20]
    assert len(list(graph)) == 20


def test_continuation_callback_can_close_cursor_without_retaining_batch_cache():
    import weakref

    class Identifier(str):
        pass

    graph = graph_rows()
    cursor_holder = []
    references = []

    def allocate(cls, value):
        if cursor_holder:
            cursor_holder[0].close()
        result = str.__new__(cls, value)
        references.append(weakref.ref(result))
        return result

    factories = NativeTermFactories(Identifier, BNode, restore, allocate, True)
    first, cursor = graph.store.backend._native.read_pattern((None, None, None), None, 2, factories)
    assert cursor is not None
    cursor_holder.append(cursor)
    batch, _ = cursor.next_batch()
    assert batch and all(type(row[1]) is Identifier for row in batch)
    assert cursor.next_batch() == ([], True)
    del first, batch
    assert all(reference() is None for reference in references)


def test_late_cached_factory_failure_releases_batch_and_closes_continuation():
    import weakref

    class Identifier(str):
        pass

    graph = graph_rows(400)
    references = []
    failure = ValueError("late cached factory failure")
    failing = False
    calls = 0

    def allocate(cls, value):
        nonlocal calls
        calls += 1
        if failing and calls == 3:
            raise failure
        result = str.__new__(cls, value)
        references.append(weakref.ref(result))
        return result

    factories = NativeTermFactories(Identifier, BNode, restore, allocate, True)
    rows, cursor = graph.store.backend._native.read_pattern((None, None, None), None, 3, factories)
    assert cursor is not None and len(rows) == 128
    del rows
    assert all(reference() is None for reference in references)
    references.clear()
    calls = 0
    failing = True
    with pytest.raises(ValueError) as caught:
        cursor.next_batch()
    assert caught.value is failure
    assert len(references) == 2  # first row populated both position caches
    # The exception traceback may retain its Python callback frame, but that
    # frame did not receive any prior cached result. Rust's partial output and
    # cache owners must have been released before the original error escaped.
    assert all(reference() is None for reference in references)
    assert cursor.next_batch() == ([], True)
    assert len(graph) == len(list(graph)) == 400


def test_threshold_does_not_activate_for_three_row_or_partial_batches():
    calls = []

    def allocate(cls, value):
        calls.append(value)
        return str.__new__(cls, value)

    factories = NativeTermFactories(URIRef, BNode, restore, allocate, True)
    batches = scan(graph_rows(131), factories)
    assert [len(batch) for batch in batches] == [128, 3]
    assert len(calls) == 4
    assert batches[0][0][1] is batches[0][-1][1]
    assert len({id(row[1]) for row in batches[1]}) == 3
    calls.clear()
    assert sum(map(len, scan(graph_rows(3), factories))) == 3
    assert len(calls) == 3
