"""Inference exposes only committed additions, retaining exact graph scope."""

from importlib.resources import files

import pytest
from rdflib import Graph as ReferenceGraph
from rdflib import Literal, Namespace
from rdflib.namespace import XSD
from rdflib_turbo import Dataset

PREFIX = "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n"
SCHEMA = """
<urn:Class> rdfs:subClassOf <urn:Parent> .
<urn:item> a <urn:Class>; <urn:label> "original" .
"""


@pytest.mark.parametrize("delta", [False, True])
def test_rdfs_substitutes_equal_string_values_preserving_the_replacement_terms(delta):
    vocabulary = Namespace("urn:literal-values:")
    values = [
        Literal("same"),
        Literal("same", lang="en"),
        Literal("same", lang="fr"),
        *(
            Literal("same", datatype=datatype)
            for datatype in (XSD.string, XSD.normalizedString, XSD.token, XSD.language, XSD.anyURI)
        ),
    ]
    original = ReferenceGraph()
    for index, value in enumerate(values):
        original.add((vocabulary[f"s{index}"], vocabulary.label, value))
    original.add(
        (vocabulary.unknown, vocabulary.label, Literal("same", datatype=vocabulary.opaque))
    )
    native = Dataset.from_rdflib(original)
    additions = native.infer_delta(inference="rdfs") if delta else native.infer(inference="rdfs")
    actual = native.to_rdflib().default_graph
    for index in range(len(values)):
        assert set(actual.objects(vocabulary[f"s{index}"], vocabulary.label)) == set(values)
    assert set(actual.objects(vocabulary.unknown, vocabulary.label)) == {
        Literal("same", datatype=vocabulary.opaque)
    }
    if delta:
        assert not set(additions.to_rdflib().default_graph) & set(original)


def statements(dataset):
    return set(dataset.serialize().splitlines())


@pytest.mark.parametrize("mode", ["rdfs", "owlrl", "both"])
@pytest.mark.parametrize("layout", ["default", "named", "mixed", "existing-destination"])
@pytest.mark.parametrize("destination", [None, "urn:inference"])
def test_inference_delta_matches_full_inference(mode, layout, destination):
    if layout == "default":
        source = Dataset().parse(data=PREFIX + SCHEMA)
    elif layout == "named":
        source = Dataset().parse(data=PREFIX + "<urn:input> { " + SCHEMA + " }", format="trig")
    elif layout == "mixed":
        source = Dataset().parse(
            data=PREFIX + SCHEMA + '<urn:input> { <urn:other> <urn:p> "named" . }', format="trig"
        )
    else:
        source = Dataset().parse(
            data=PREFIX + SCHEMA + "<urn:inference> { <urn:item> a <urn:Parent> . }", format="trig"
        )
    originals = statements(source)
    expected = source.copy().infer(inference=mode, destination=destination)

    delta = source.infer_delta(inference=mode, destination=destination)

    assert delta is not source
    assert statements(source) == statements(expected)
    assert statements(delta) == statements(expected) - originals
    assert dict(delta.namespaces()) == dict(source.namespaces())
    # The existing RDFS initialization can add consequences on a second call.
    # bounded: this fixed schema reaches a stable closure within four calls.
    for _ in range(4):
        originals = statements(source)
        expected = source.copy().infer(inference=mode, destination=destination)
        repeated_delta = source.infer_delta(inference=mode, destination=destination)
        assert statements(source) == statements(expected)
        assert statements(repeated_delta) == statements(expected) - originals
        if len(repeated_delta) == 0:
            break
    else:
        pytest.fail("The fixed inference fixture did not reach a stable closure")


@pytest.mark.parametrize("destination", [None, "urn:inference"])
@pytest.mark.parametrize("named", [False, True])
def test_inference_delta_failure_rolls_back_all_changes(destination, named):
    statements = (
        SCHEMA
        + "<urn:keyAlias> rdfs:subPropertyOf <http://www.w3.org/2002/07/owl#hasKey> . "
        + "<urn:Class> <urn:keyAlias> _:broken ."
    )
    source = Dataset().parse(
        data=PREFIX + ("<urn:input> { " + statements + " }" if named else statements),
        format="trig" if named else "turtle",
    )
    before = source.serialize()
    with pytest.raises(ValueError, match="Malformed RDF list"):
        source.infer_delta(destination=destination)
    assert source.serialize() == before


@pytest.mark.parametrize(
    "arguments, message",
    [({"inference": "invalid"}, "Unknown inference mode"), ({"destination": "invalid IRI"}, "IRI")],
)
def test_inference_delta_rejects_invalid_parameters_without_mutation(arguments, message):
    source = Dataset().parse(data=PREFIX + SCHEMA)
    before = source.serialize()
    with pytest.raises(ValueError, match=message):
        source.infer_delta(**arguments)
    assert source.serialize() == before


def test_installed_package_contains_typing_marker():
    assert files("rdflib_turbo").joinpath("py.typed").is_file()
