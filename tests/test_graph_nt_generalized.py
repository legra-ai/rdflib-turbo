"""RDFLib NT dispatch uses Literal.n3 in S/P and its NT quoting in O."""

import warnings

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib import Graph as ReferenceGraph
from rdflib.namespace import XSD
from rdflib_turbo import Graph


def render(graph, *, error=False):
    with warnings.catch_warnings(record=True) as observed:
        warnings.simplefilter("error" if error else "always")
        try:
            result = graph.serialize(format="nt")
        except Exception as cause:
            result = (type(cause), str(cause))
    return result, [(warning.category, str(warning.message)) for warning in observed]


@pytest.mark.parametrize("subject", [URIRef("urn:s"), BNode("s"), Literal("subject")])
@pytest.mark.parametrize("predicate", [URIRef("urn:p"), BNode("p"), Literal("predicate")])
def test_generalized_roles_use_native_scoped_nt(subject, predicate, monkeypatch):
    statement = subject, predicate, Literal("object")
    reference = ReferenceGraph().add(statement)
    native = Graph().add(statement)
    monkeypatch.setattr(native, "triples", lambda *_: pytest.fail("Python row traversal"))
    assert render(native) == render(reference)
    assert native.serialize(format="nt", encoding="utf-8") == reference.serialize(
        format="nt", encoding="utf-8"
    )


@pytest.mark.parametrize("position", range(3))
@pytest.mark.parametrize(
    "literal",
    [
        Literal('line\nend"'),
        Literal("nan", datatype=XSD.double, normalize=False),
        Literal("-inf", datatype=XSD.decimal, normalize=False),
        Literal("bad", datatype=XSD.double, normalize=False),
        Literal("text", datatype=URIRef("urn:datatype space")),
        Literal("texte", lang="fr"),
    ],
)
def test_literal_formatting_depends_on_triple_position(position, literal):
    statement = [URIRef("urn:s"), URIRef("urn:p"), Literal("object")]
    statement[position] = literal
    reference = ReferenceGraph().add(tuple(statement))
    native = Graph().add(tuple(statement))
    assert render(native) == render(reference)
    assert render(native, error=True) == render(reference, error=True)


@pytest.mark.parametrize("warning_first", [False, True])
def test_warning_and_uri_error_order_keeps_pending_owner_usable(warning_first):
    weird = Literal("bad", datatype=XSD.double, normalize=False)
    invalid = URIRef("urn:bad space")
    statement = (
        (weird, invalid, Literal("object"))
        if warning_first
        else (invalid, weird, Literal("object"))
    )
    reference = ReferenceGraph().add(statement)
    native = Graph()
    with native.transaction() as pending:
        pending.add(statement)
        assert render(pending) == render(reference)
        assert render(pending, error=True) == render(reference, error=True)
        pending.remove(statement)
        pending.add((URIRef("urn:good"), URIRef("urn:p"), Literal("usable")))
    assert set(native) == {(URIRef("urn:good"), URIRef("urn:p"), Literal("usable"))}


@pytest.mark.parametrize("position", range(3))
def test_empty_datatype_serialization_preserves_stored_identity(position):
    literal = Literal("value", datatype=URIRef(""), normalize=False)
    statement = [URIRef("urn:s"), URIRef("urn:p"), Literal("object")]
    statement[position] = literal
    reference = ReferenceGraph().add(tuple(statement))
    native = Graph().add(tuple(statement))
    assert render(native) == render(reference)
    retained = next(iter(native))[position]
    assert isinstance(retained, Literal)
    assert retained.datatype is not None
    assert str(retained.datatype) == ""
