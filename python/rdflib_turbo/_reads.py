"""Typed, projected reads over graph-scoped native storage cursors."""

from rdflib import BNode, Literal, URIRef
from rdflib.paths import Path

from ._literal import restore
from ._native import NativeInputTerms, NativeTermFactories
from .store import Store

_TERM_FACTORIES = NativeTermFactories(URIRef, BNode, restore, str.__new__, True)
_INPUT_TERMS = NativeInputTerms(URIRef, BNode, Literal)


def encode(value):
    if value is None:
        return None
    if isinstance(value, URIRef):
        return ("uri", value, None, None)
    if isinstance(value, BNode):
        return ("bnode", value, None, None)
    if isinstance(value, Literal):
        return ("literal", value, value.datatype, value.language)
    raise TypeError(f"Unsupported RDF term: {type(value).__name__}")


def encode_graph(store, context):
    identifier = context.identifier if hasattr(context, "identifier") else context
    return None if identifier == store.default_identifier else encode(identifier)


def physical_rows(store, pattern, context):
    if context is None:
        scope = ("any", None)
    else:
        graph = encode_graph(store, context)
        scope = ("default", None) if graph is None else ("graph", graph)
    batch, cursor = store.backend._native.read_quads(
        tuple(map(encode, pattern)), scope, _TERM_FACTORIES
    )
    try:
        while True:
            for subject, predicate, obj, graph in batch:
                yield subject, predicate, obj, store.default_identifier if graph is None else graph
            if cursor is None:
                break
            batch, finished = cursor.next_batch()
            if finished:
                cursor = None
    finally:
        if cursor is not None:
            cursor.close()


def union_rows(store, pattern, context=None):
    if context is None:
        scope = ("any", None)
    else:
        graph = encode_graph(store, context)
        scope = ("default", None) if graph is None else ("graph", graph)
    batch, cursor = store.backend._native.read_union(
        tuple(map(encode, pattern)), _TERM_FACTORIES, scope=scope
    )
    try:
        while True:
            yield from batch
            if cursor is None:
                break
            batch, finished = cursor.next_batch()
            if finished:
                cursor = None
    finally:
        if cursor is not None:
            cursor.close()


def context_names(store, cursor):
    try:
        while True:
            batch, finished = cursor.next_batch()
            for identifier in batch:
                yield store.default_identifier if identifier is None else identifier
            if finished:
                return
    finally:
        cursor.close()


def scoped_rows(store, pattern, context, positions=(0, 1, 2)):
    graph = encode_graph(store, context)
    # Bound terms already belong to the caller: transport/decode only unknown outputs.
    count = len(positions)
    first = positions[0]
    mask = 7 if count == 3 else (1 << first) | (1 << positions[-1])
    output_mask = mask & (
        (pattern[0] is None) | ((pattern[1] is None) << 1) | ((pattern[2] is None) << 2)
    )
    batch, cursor = store.backend._native.read_pattern(
        (encode(pattern[0]), encode(pattern[1]), encode(pattern[2])),
        graph,
        output_mask,
        _TERM_FACTORIES,
    )
    try:
        while True:
            if count == 1:
                for row in batch:
                    yield row[first]
            elif count == 2:
                second = positions[1]
                for row in batch:
                    yield row[first], row[second]
            elif output_mask == 7:
                # Every SPO slot is unbound: native tuples already contain the complete result.
                yield from batch
            else:
                for subject, predicate, obj in batch:
                    yield (
                        pattern[0] if subject is None else subject,
                        pattern[1] if predicate is None else predicate,
                        pattern[2] if obj is None else obj,
                    )
            if cursor is None:
                break
            batch, finished = cursor.next_batch()
            if finished:
                cursor = None
    finally:
        if cursor is not None:
            cursor.close()


class ProjectedReads:
    """Avoid constructing unused RDF terms for ordinary graph projections."""

    def __contains__(self, pattern):
        if (
            type(self).triples is not ProjectedReads.triples
            or type(self).__getattribute__ is not object.__getattribute__
            or "triples" in self.__dict__
            or not isinstance(self.store.backend, Store)
            or isinstance(pattern[1], Path)
            or pattern[0] is None
            or pattern[1] is None
            or pattern[2] is None
        ):
            return super().__contains__(pattern)
        return self.store.backend._native.contains_pattern(pattern, self, _INPUT_TERMS)

    def triples(self, pattern):
        if isinstance(pattern[1], Path) or not isinstance(self.store.backend, Store):
            yield from super().triples(pattern)
        else:
            yield from scoped_rows(self.store, pattern, self)

    def _project(self, pattern, positions):
        if isinstance(self.store.backend, Store):
            yield from scoped_rows(self.store, pattern, self, positions)
            return
        # Explicit transactions retain their owner-thread read protocol.
        rows = self.triples(pattern)
        try:
            for row in rows:
                yield row[positions[0]] if len(positions) == 1 else tuple(row[i] for i in positions)
        finally:
            rows.close()

    def _custom_triples(self):
        return type(self).triples is not ProjectedReads.triples

    def subjects(self, predicate=None, object=None, unique=False):
        if (
            unique
            or isinstance(object, list)
            or isinstance(predicate, Path)
            or self._custom_triples()
        ):
            yield from super().subjects(predicate, object, unique)
        else:
            yield from self._project((None, predicate, object), (0,))

    def predicates(self, subject=None, object=None, unique=False):
        if unique or self._custom_triples():
            yield from super().predicates(subject, object, unique)
        else:
            yield from self._project((subject, None, object), (1,))

    def objects(self, subject=None, predicate=None, unique=False):
        if (
            unique
            or isinstance(subject, list)
            or isinstance(predicate, Path)
            or self._custom_triples()
        ):
            yield from super().objects(subject, predicate, unique)
        else:
            yield from self._project((subject, predicate, None), (2,))

    def subject_objects(self, predicate=None, unique=False):
        if unique or isinstance(predicate, Path) or self._custom_triples():
            yield from super().subject_objects(predicate, unique)
        else:
            yield from self._project((None, predicate, None), (0, 2))

    def predicate_objects(self, subject=None, unique=False):
        if unique or self._custom_triples():
            yield from super().predicate_objects(subject, unique)
        else:
            yield from self._project((subject, None, None), (1, 2))

    def subject_predicates(self, object=None, unique=False):
        if unique or self._custom_triples():
            yield from super().subject_predicates(object, unique)
        else:
            yield from self._project((None, None, object), (0, 1))
