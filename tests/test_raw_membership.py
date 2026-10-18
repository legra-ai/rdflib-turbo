"""Raw membership must preserve Python boundary behavior without transport tuples."""

import gc
import weakref

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib.namespace import XSD
from rdflib_turbo import Graph, _reads

TRIPLE = (URIRef("urn:s"), URIRef("urn:p"), Literal("value"))


def test_membership_avoids_python_transport_encoding(monkeypatch):
    graph = Graph()
    graph.add(TRIPLE)

    def forbidden(*args):
        raise AssertionError("membership entered Python tuple encoding")

    monkeypatch.setattr(_reads, "encode", forbidden)
    monkeypatch.setattr(_reads, "encode_graph", forbidden)
    assert TRIPLE in graph
    assert (TRIPLE[0], TRIPLE[1], Literal("absent")) not in graph


def record_callbacks(bad_datatype=False):
    events = []

    class Identifier(URIRef):
        __hash__ = URIRef.__hash__

        def __eq__(self, other):
            events.append("eq")
            return super().__eq__(other)

    class Value(Literal):
        @property
        def datatype(self):
            events.append("datatype")
            return object() if bad_datatype else None

        @property
        def language(self):
            events.append("language")
            return None

    class Owner(Graph):
        @property
        def store(self):
            events.append("store")
            return super().store

        @property
        def identifier(self):
            events.append("identifier")
            return super().identifier

    class Pattern:
        def __getitem__(self, index):
            events.append(f"item{index}")
            return (TRIPLE[0], TRIPLE[1], value)[index]

    graph = Owner(identifier=Identifier("urn:graph"))
    graph.add(TRIPLE)
    value = Value("value")
    events.clear()
    try:
        result = Pattern() in graph
        error = None
    except Exception as cause:
        result = None
        error = (type(cause).__name__, str(cause), cause.__cause__, cause.__context__)
    return events, result, error


def test_callback_order_matches_original_transport_boundary():
    events, result, error = record_callbacks()
    assert result is True and error is None
    assert events == [
        "store",
        "item1",
        "item0",
        "item1",
        "item2",
        "store",
        "item0",
        "item1",
        "item2",
        "datatype",
        "language",
        "store",
        "identifier",
        "identifier",
        "eq",
    ]


def test_invalid_metadata_is_extracted_after_all_graph_callbacks():
    events, result, error = record_callbacks(True)
    assert result is None and error == (
        "TypeError",
        "'object' object is not an instance of 'str'",
        None,
        None,
    )
    assert events == [
        "store",
        "item1",
        "item0",
        "item1",
        "item2",
        "store",
        "item0",
        "item1",
        "item2",
        "datatype",
        "language",
        "store",
        "identifier",
        "identifier",
        "eq",
    ]


@pytest.mark.parametrize(
    "identifier", [None, URIRef("urn:named"), BNode("context"), URIRef("relative")]
)
def test_raw_membership_preserves_context_and_exact_literal_identity(identifier):
    graph = Graph(identifier=identifier)
    values = [
        Literal("01", datatype=XSD.integer, normalize=False),
        Literal("1", datatype=XSD.integer, normalize=False),
        Literal("x"),
        Literal("x", datatype=XSD.string),
        Literal("oui", lang="fr"),
    ]
    for value in values:
        graph.add((TRIPLE[0], TRIPLE[1], value))
    for value in values:
        assert (TRIPLE[0], TRIPLE[1], value) in graph
    assert (TRIPLE[0], TRIPLE[1], Literal("missing")) not in graph


def test_raw_membership_getters_may_close_owner_before_snapshot():
    graph = Graph()
    graph.add(TRIPLE)

    class Closing(Literal):
        @property
        def datatype(self):
            graph.store.backend.close()
            return None

        @property
        def language(self):
            return None

    with pytest.raises(RuntimeError, match="closed"):
        (TRIPLE[0], TRIPLE[1], Closing("value")) in graph


def test_input_descriptor_visits_class_cycles():
    from rdflib_turbo._native import NativeInputTerms

    class Identifier(URIRef):
        pass

    descriptor = NativeInputTerms(Identifier, BNode, Literal)
    Identifier.descriptor = descriptor
    reference = weakref.ref(Identifier)
    del descriptor, Identifier
    gc.collect()
    assert reference() is None


def test_ephemeral_metadata_is_released_after_lookup_and_can_reenter():
    graph = Graph()
    datatype = URIRef("urn:datatype")
    graph.add((TRIPLE[0], TRIPLE[1], Literal("value", datatype=datatype)))
    references = []
    released = []

    class Metadata(str):
        def __del__(self):
            released.append(len(graph))

    class Value(Literal):
        @property
        def datatype(self):
            result = Metadata("urn:datatype")
            references.append(weakref.ref(result))
            return result

        @property
        def language(self):
            return None

    assert (TRIPLE[0], TRIPLE[1], Value("value")) in graph
    assert released == [1]
    assert all(reference() is None for reference in references)


def test_instance_class_hook_is_honored_before_string_extraction():
    graph = Graph()
    graph.add(TRIPLE)
    events = []

    class PretendIdentifier:
        @property
        def __class__(self):
            events.append("class")
            return URIRef

    with pytest.raises(TypeError, match="str"):
        (PretendIdentifier(), TRIPLE[1], TRIPLE[2]) in graph
    assert events == ["class"]


def test_typeerror_uses_unqualified_type_name_without_calling_repr():
    graph = Graph()

    class Unsupported:
        def __repr__(self):
            raise AssertionError("unsupported-term errors must not call repr")

    with pytest.raises(TypeError, match=r"^Unsupported RDF term: Unsupported$"):
        (Unsupported(), TRIPLE[1], TRIPLE[2]) in graph


def test_partial_raw_inputs_release_metadata_if_later_index_raises():
    graph = Graph()
    graph.add(TRIPLE)
    failure = LookupError("pattern callback failed")
    references = []
    released = []

    class Metadata(str):
        def __del__(self):
            released.append(len(graph))

    class Subject(Literal):
        @property
        def datatype(self):
            metadata = Metadata("urn:datatype")
            references.append(weakref.ref(metadata))
            return metadata

        @property
        def language(self):
            return None

    subject = Subject("value")

    class Pattern:
        def __init__(self):
            self.calls = 0

        def __getitem__(self, index):
            self.calls += 1
            if self.calls == 7:
                raise failure
            return (subject, TRIPLE[1], TRIPLE[2])[index]

    with pytest.raises(LookupError) as caught:
        Pattern() in graph
    assert caught.value is failure
    assert released == [1]
    assert all(reference() is None for reference in references)


def test_input_descriptor_preserves_metaclass_instance_checks(monkeypatch):
    from rdflib_turbo._native import NativeInputTerms

    events = []

    class Meta(type):
        def __instancecheck__(cls, value):
            events.append(type(value).__name__)
            return isinstance(value, URIRef)

    class Identifier(str, metaclass=Meta):
        pass

    graph = Graph()
    graph.add(TRIPLE)
    descriptor = NativeInputTerms(Identifier, BNode, Literal)
    monkeypatch.setattr(_reads, "_INPUT_TERMS", descriptor)
    assert TRIPLE in graph
    assert events == ["URIRef", "URIRef", "Literal"]


@pytest.mark.parametrize(
    "types", [(int, BNode, Literal), (URIRef, int, Literal), (URIRef, BNode, int)]
)
def test_input_descriptor_rejects_non_string_types(types):
    from rdflib_turbo._native import NativeInputTerms

    with pytest.raises(TypeError, match="string subclasses"):
        NativeInputTerms(*types)


def test_graph_comparison_releases_rhs_before_truth_callback():
    from rdflib_turbo.graph import RDFLibStore

    events = []
    tracking = False

    class Truth:
        def __bool__(self):
            events.append("bool")
            return False

    class Identifier(URIRef):
        __hash__ = URIRef.__hash__

        def __eq__(self, other):
            if tracking:
                events.append("eq")
                return Truth()
            return super().__eq__(other)

    class Temporary(URIRef):
        def __del__(self):
            events.append("rhs-released")

    class ObservedStore(RDFLibStore):
        @property
        def default_identifier(self):
            return Temporary("urn:default") if tracking else self.original_identifier

        @default_identifier.setter
        def default_identifier(self, value):
            self.original_identifier = value

    graph = Graph(store=ObservedStore(), identifier=Identifier("urn:graph"))
    graph.add(TRIPLE)
    tracking = True
    assert TRIPLE in graph
    tracking = False
    assert events == ["eq", "rhs-released", "bool"]


@pytest.mark.parametrize("slot", range(3))
def test_changing_pattern_to_none_keeps_original_tuple_typeerror(slot):
    graph = Graph().add(TRIPLE)

    class Pattern:
        def __init__(self):
            self.calls = [0, 0, 0]

        def __getitem__(self, index):
            self.calls[index] += 1
            if index == slot and self.calls[index] > (1, 2, 1)[index]:
                return None
            return TRIPLE[index]

    with pytest.raises(TypeError) as caught:
        Pattern() in graph
    assert str(caught.value) == "'None' is not an instance of 'tuple'"
    assert caught.value.__cause__ is None and caught.value.__context__ is None
