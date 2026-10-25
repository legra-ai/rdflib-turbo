"""SingleLine is an explicit draft extension beyond pySHACL 0.40.1."""

import pytest
from pyshacl import validate as original_validate
from pyshacl_turbo import compile_shapes, validate
from rdflib import RDF, BNode, Graph, Literal, URIRef
from rdflib.compare import isomorphic
from rdflib.namespace import SH


def inputs(property_shape, separator):
    data = Graph()
    subject, predicate = URIRef("urn:single-line:s"), URIRef("urn:single-line:p")
    value = Literal(f"first{separator}second")
    data.add((subject, predicate, value))
    shapes, shape = Graph(), BNode("shape")
    shapes.add((shape, RDF.type, SH.NodeShape))
    shapes.add((shape, SH.targetNode, subject if property_shape else value))
    selected = shape
    if property_shape:
        selected = BNode("property")
        shapes.add((shape, SH.property, selected))
        shapes.add((selected, SH.path, predicate))
    return data, shapes, selected, value


@pytest.mark.parametrize("property_shape", [False, True])
@pytest.mark.parametrize("enabled", [False, True])
@pytest.mark.parametrize("separator", ["\n", "\v", "\f", "\r", "\r\n"])
def test_selected_draft_extension_is_distinct_from_the_pinned_reference(
    property_shape, enabled, separator
):
    data, shapes, selected, value = inputs(property_shape, separator)
    shapes.add((selected, URIRef(str(SH) + "singleLine"), Literal(enabled)))
    original = original_validate(data, shacl_graph=shapes, advanced=True)
    observed = validate(data, shacl_graph=shapes, advanced=True)
    assert original[0] is True  # That reference predates this selected draft feature.
    assert observed[0] is not enabled
    if enabled:
        results = list(observed[1].subjects(RDF.type, SH.ValidationResult))
        assert len(results) == 1
        assert observed[1].value(results[0], SH.value) == value
        assert observed[1].value(results[0], SH.sourceConstraintComponent) == URIRef(
            str(SH) + "SingleLineConstraintComponent"
        )
    else:
        assert isomorphic(original[1], observed[1])
        assert original[2] == observed[2]


@pytest.mark.parametrize("property_shape", [False, True])
@pytest.mark.parametrize("parameters", [("not boolean",), (1,), (True, False)])
def test_draft_declarations_reject_invalid_or_repeated_values_reference_ignores(
    property_shape, parameters
):
    data, shapes, selected, _ = inputs(property_shape, "\n")
    for parameter in parameters:
        shapes.add((selected, URIRef(str(SH) + "singleLine"), Literal(parameter)))
    assert original_validate(data, shacl_graph=shapes)[0] is True
    with pytest.raises(ValueError, match="singleLine"):
        compile_shapes(shapes)
