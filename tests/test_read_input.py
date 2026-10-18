"""Read filters pin accepted Python text only for the native call."""

import gc
import weakref

import pytest
from rdflib import BNode, URIRef
from rdflib_turbo import Graph
from rdflib_turbo._literal import restore
from rdflib_turbo._native import NativeTermFactories

FACTORIES = NativeTermFactories(URIRef, BNode, restore, str.__new__)


class Text(str):
    def __str__(self):
        raise AssertionError("native extraction invoked Python __str__")


def test_temporary_filter_is_not_retained_by_the_snapshot():
    graph = Graph().parse(
        data="\n".join(f'<urn:s{i}> <urn:p> "v" .' for i in range(400)), format="nt"
    )

    def start():
        value = Text("urn:p")
        reference = weakref.ref(value)
        rows, cursor = graph.store.backend._native.read_pattern(
            (None, (Text("uri"), value, None, None), None), None, 1, FACTORIES
        )
        return rows, cursor, reference

    rows, cursor, reference = start()
    gc.collect()
    assert reference() is None
    assert cursor is not None
    graph.add((URIRef("urn:after"), URIRef("urn:p"), URIRef("urn:o")))
    finished = False
    while not finished:
        batch, finished = cursor.next_batch()
        rows.extend(batch)
    assert len(rows) == 400
    assert all(row[0] != URIRef("urn:after") for row in rows)


@pytest.mark.parametrize(
    "object",
    [
        ("literal", "01", "http://www.w3.org/2001/XMLSchema#integer", None),
        ("literal", "x", None, None),
        ("literal", "x", "http://www.w3.org/2001/XMLSchema#string", None),
        ("literal", "bonjour", None, "fR"),
        ("literal", "日本語🙂" * 1024, None, None),
        ("bnode", "0123", None, None),
    ],
)
def test_read_filter_text_views_preserve_exact_metadata_and_scope(object):
    graph = Graph()
    native = graph.store.backend._native
    subject, predicate, context = (
        ("uri", "relative", None, None),
        ("uri", "urn:p", None, None),
        ("bnode", "0123", None, None),
    )
    native.insert_terms(subject, predicate, object, context)

    def pinned(term):
        return tuple(Text(value) if value is not None else None for value in term)

    rows, cursor = native.read_pattern(
        (pinned(subject), pinned(predicate), pinned(object)), pinned(context), 0, FACTORIES
    )
    assert rows == [(None, None, None)] and cursor is None
    assert native.read_pattern((None, None, pinned(object)), None, 0, FACTORIES) == ([], None)


@pytest.mark.parametrize(
    "term",
    [
        ("unknown", "x", None, None),
        ("uri", "urn:x", "urn:datatype", None),
        ("bnode", "x", None, "en"),
        ("literal", "x", "urn:type", "en"),
        ("literal", "x", None, "not a language"),
    ],
)
def test_read_filter_shared_validator_rejects_invalid_metadata(term):
    graph = Graph()
    with pytest.raises(ValueError):
        graph.store.backend._native.read_pattern((None, None, term), None, 0, FACTORIES)
    assert len(graph) == 0


def test_read_filter_surrogate_error_is_preserved():
    graph = Graph()
    with pytest.raises(UnicodeEncodeError):
        graph.store.backend._native.read_pattern(
            (None, None, ("literal", "\ud800", None, None)), None, 0, FACTORIES
        )
