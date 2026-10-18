"""One immutable validated descriptor owns factory handles and their GC edges."""

import gc
import weakref

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib_turbo import Graph
from rdflib_turbo._literal import restore


def descriptor(literal=restore):
    from rdflib_turbo._native import NativeTermFactories

    return NativeTermFactories(URIRef, BNode, literal, str.__new__)


def graph_with_rows():
    return Graph().parse(
        data="\n".join(f'<urn:s{i}> <urn:p> "v" .' for i in range(400)), format="nt"
    )


def test_native_read_rejects_displaced_factory_tuple():
    graph = Graph().add((URIRef("urn:s"), URIRef("urn:p"), Literal("value")))
    with pytest.raises(TypeError):
        graph.store.backend._native.read_pattern(
            (None, None, None), None, 7, (URIRef, BNode, restore, str.__new__)
        )


def test_standard_descriptor_is_created_once_and_remains_immutable(monkeypatch):
    from rdflib_turbo import _reads
    from rdflib_turbo._native import NativeTermFactories

    factories = _reads._TERM_FACTORIES
    assert type(factories) is NativeTermFactories
    with pytest.raises(AttributeError):
        factories.literal = restore

    def forbidden(*args, **kwargs):
        raise AssertionError("read reconstructed factory descriptor")

    monkeypatch.setattr(_reads, "NativeTermFactories", forbidden)
    graph = graph_with_rows()
    assert len(list(graph)) == 400
    assert len(list(graph.objects())) == 400


@pytest.mark.parametrize(
    "arguments",
    [
        (42, BNode, restore, str.__new__),
        (int, BNode, restore, str.__new__),
        (URIRef, int, restore, str.__new__),
        (URIRef, BNode, 42, str.__new__),
        (URIRef, BNode, restore, 42),
    ],
)
def test_descriptor_rejects_invalid_handles_at_construction(arguments):
    from rdflib_turbo._native import NativeTermFactories

    with pytest.raises(TypeError):
        NativeTermFactories(*arguments)


def test_shared_descriptor_has_one_gc_edge_per_cursor():
    graph = graph_with_rows()

    class Callback:
        def __init__(self):
            self.cursors = []

        def __call__(self, *args):
            return restore(*args)

    def cycle():
        callback = Callback()
        factories = descriptor(callback)
        for _ in range(2):
            _, cursor = graph.store.backend._native.read_pattern(
                (None, None, None), None, 4, factories
            )
            callback.cursors.append(cursor)
        return factories, weakref.ref(callback)

    factories, callback = cycle()
    gc.collect()
    assert callback() is not None
    assert len(callback().cursors) == 2
    del factories
    gc.collect()
    assert callback() is None


def test_factory_error_does_not_poison_reusable_descriptor():
    graph = graph_with_rows()
    failure = ValueError("descriptor factory failed")
    failing = True
    calls = 0

    def literal(*args):
        nonlocal calls
        calls += 1
        if failing:
            raise failure
        return restore(*args)

    factories = descriptor(literal)
    with pytest.raises(ValueError) as captured:
        graph.store.backend._native.read_pattern((None, None, None), None, 4, factories)
    assert captured.value is failure and calls == 1
    failing = False
    rows, cursor = graph.store.backend._native.read_pattern((None, None, None), None, 4, factories)
    while cursor is not None:
        batch, finished = cursor.next_batch()
        rows.extend(batch)
        if finished:
            cursor = None
    assert len(rows) == 400 and calls == 401
    assert all(row[2].value == "v" for row in rows)
