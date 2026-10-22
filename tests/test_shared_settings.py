"""The native SHACL settings boundary consumes one strict typed JSON object."""

import pytest
from pyshacl_turbo.rdf import decode
from rdflib.compare import isomorphic
from rdflib_turbo import Dataset
from rdflib_turbo._native import SharedShapes

SHAPES = """@prefix sh:<http://www.w3.org/ns/shacl#>.
<urn:S> sh:targetNode <urn:a>; sh:property [sh:path <urn:p>; sh:minCount 1]."""


@pytest.mark.parametrize(
    ("field", "value"),
    [
        ("iterate_rules", "false"),
        ("max_rule_iterations", "10"),
        ("max_validation_depth", "15"),
        ("allow_infos", "false"),
        ("allow_warnings", "false"),
        ("abort_on_first", "false"),
        ("focus_nodes", "[]"),
        ("use_shapes", "[]"),
        ("inference", '"none"'),
        ("data_namespaces", "[]"),
    ],
)
def test_duplicate_known_settings_fail_without_mutating_receiver(field, value):
    data = Dataset().parse(data="<urn:a> <urn:p> <urn:b>.")
    before = data.serialize(), tuple(data.namespaces())
    settings = f'{{"{field}":{value},"{field}":{value}}}'
    with pytest.raises(ValueError, match="duplicate field"):
        SharedShapes(SHAPES, False, []).validate(data._native, settings, True)
    assert (data.serialize(), tuple(data.namespaces())) == before


@pytest.mark.parametrize(
    "settings",
    [
        "[]",
        "[false,10,15,false,false,false,[],[]]",
        "null",
        "true",
        '"object"',
        "0",
        "{",
        "{} {}",
        '{"unknown":false}',
        '{"allow_infos":0}',
        '{"focus_nodes":[false]}',
        '{"max_rule_iterations":-1}',
        '{"inference":true}',
        '{"inference":"OWL"}',
        '{"data_namespaces":[["ex"]]}',
        '{"data_namespaces":[["ex",false]]}',
    ],
)
def test_nonobjects_malformed_unknown_and_wrong_typed_settings_fail(settings):
    with pytest.raises(ValueError):
        SharedShapes(SHAPES, False, []).validate(Dataset()._native, settings, False)


def test_all_valid_settings_and_default_settings_have_same_report():
    data = Dataset().parse(data="@prefix custom:<urn:>. custom:a custom:p custom:b.")
    program = SharedShapes(SHAPES, False, [])
    complete = """{"iterate_rules":false,"max_rule_iterations":10,
    "max_validation_depth":15,"allow_infos":false,"allow_warnings":false,
    "abort_on_first":false,"focus_nodes":[],"use_shapes":[]}"""
    actual = program.validate(data._native, complete, False)
    default = program.validate(data._native, "{}", False)
    assert actual[0] == default[0]
    assert actual[2:] == default[2:]
    assert isomorphic(
        decode(actual[1], {}, preserve_ids=True), decode(default[1], {}, preserve_ids=True)
    )
