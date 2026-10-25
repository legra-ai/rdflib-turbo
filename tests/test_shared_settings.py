"""The native SHACL settings boundary consumes one strict typed JSON object."""

import pytest
from pyshacl_turbo.rdf import decode
from rdflib.compare import isomorphic
from rdflib_turbo._native import NativeDataset, SharedShapes
from rdflib_turbo._native import shacl as _native_shacl


def load_dataset(data, source):
    data.load(source, "turtle", None, True)
    return data


def native_dataset(source):
    return load_dataset(NativeDataset(), source)


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
    data = native_dataset("<urn:a> <urn:p> <urn:b>.")
    before = data.dump("nquads"), tuple(data.namespaces())
    settings = f'{{"{field}":{value},"{field}":{value}}}'
    with pytest.raises(ValueError, match="duplicate field"):
        SharedShapes(
            SHAPES,
            False,
            [],
            _policy=_native_shacl._REGEX_POLICY,
            _comparison_policy=_native_shacl._VALUE_COMPARISON_POLICY,
        ).validate(data, settings, True)
    assert (data.dump("nquads"), tuple(data.namespaces())) == before


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
        SharedShapes(
            SHAPES,
            False,
            [],
            _policy=_native_shacl._REGEX_POLICY,
            _comparison_policy=_native_shacl._VALUE_COMPARISON_POLICY,
        ).validate(NativeDataset(), settings, False)


def test_all_valid_settings_and_default_settings_have_same_report():
    data = native_dataset("@prefix custom:<urn:>. custom:a custom:p custom:b.")
    program = SharedShapes(
        SHAPES,
        False,
        [],
        _policy=_native_shacl._REGEX_POLICY,
        _comparison_policy=_native_shacl._VALUE_COMPARISON_POLICY,
    )
    complete = """{"iterate_rules":false,"max_rule_iterations":10,
    "max_validation_depth":15,"allow_infos":false,"allow_warnings":false,
    "abort_on_first":false,"focus_nodes":[],"use_shapes":[]}"""
    actual = program.validate(data, complete, False)
    default = program.validate(data, "{}", False)
    assert actual[0] == default[0]
    assert actual[2:] == default[2:]
    assert isomorphic(
        decode(actual[1], {}, preserve_ids=True), decode(default[1], {}, preserve_ids=True)
    )
