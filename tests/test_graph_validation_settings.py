"""Scoped settings retain strict validation while capturing owner namespaces."""

import json

import pytest
from rdflib_turbo import Graph
from rdflib_turbo._namespaces import NamespaceBindings
from rdflib_turbo._native import SharedShapes
from rdflib_turbo._validation import _bound_settings


def test_scoped_settings_capture_authoritative_namespace_pairs():
    result = json.loads(
        _bound_settings('{"inference":"rdfs"}', NamespaceBindings([("ex", "urn:data:")]))
    )
    assert result == {"inference": "rdfs", "data_namespaces": [["ex", "urn:data:"]]}


@pytest.mark.parametrize(
    "settings",
    [
        "[]",
        "null",
        "0",
        '{"data_namespaces":[]}',
        '{"inference":"rdfs","inference":"owlrl"}',
        '{"unknown":{"nested":true,"nested":false}}',
    ],
)
def test_scoped_settings_fail_without_accepting_pending_data(settings):
    source = Graph()
    program = SharedShapes("", False, [])
    with pytest.raises(ValueError), source.transaction() as pending:
        pending.parse(data="<urn:s> <urn:p> <urn:o>.")
        with pending._validate_shared_shapes(program, settings, True):
            pytest.fail("Invalid scoped settings reached a report")
    assert len(source) == 0
    source.close()


@pytest.mark.parametrize("binding_failure", [False, True])
def test_diagnostic_namespace_binding_is_inside_graph_acceptance(binding_failure, monkeypatch):
    source = Graph().parse(
        data='<urn:s> <urn:p> "bad"^^<http://www.w3.org/2001/XMLSchema#integer>.'
    )
    program = SharedShapes("", False, [])
    before = source.serialize(format="nt"), tuple(source.namespaces())
    error_namespace = "http://www.daml.org/2002/03/agents/agent-ont#"

    def reject_binding(*args, **kwargs):
        raise RuntimeError("synthetic diagnostic namespace binding failure")

    with monkeypatch.context() as patch:
        if binding_failure:
            patch.setattr(Graph, "bind", reject_binding)
        try:
            with source._validate_shared_shapes(program, '{"inference":"owlrl"}', True) as result:
                assert result[0]
                assert not any(
                    str(namespace) == error_namespace for _, namespace in source.namespaces()
                )
        except RuntimeError as failure:
            assert binding_failure and "synthetic diagnostic namespace binding failure" in str(
                failure
            )
        else:
            assert not binding_failure
    if binding_failure:
        assert (source.serialize(format="nt"), tuple(source.namespaces())) == before
    else:
        assert any(
            prefix == "err" and str(namespace) == error_namespace
            for prefix, namespace in source.namespaces()
        )
    source.close()


def test_reentrant_diagnostic_binding_rechecks_original_namespace_revision(monkeypatch):
    source = Graph().parse(
        data='<urn:s> <urn:p> "bad"^^<http://www.w3.org/2001/XMLSchema#integer>.'
    )
    program = SharedShapes("", False, [])
    before = source.serialize(format="nt")
    original_bind = Graph.bind

    def reentrant_bind(graph, prefix, namespace, *args, **kwargs):
        if str(namespace) == "http://www.daml.org/2002/03/agents/agent-ont#":
            original_bind(source, "callback", "urn:callback:")
        return original_bind(graph, prefix, namespace, *args, **kwargs)

    monkeypatch.setattr(Graph, "bind", reentrant_bind)
    with pytest.raises(RuntimeError, match="namespaces changed"):
        with source._validate_shared_shapes(program, '{"inference":"owlrl"}', True):
            pass
    assert source.serialize(format="nt") == before
    assert str(dict(source.namespaces())["callback"]) == "urn:callback:"
    assert "err" not in dict(source.namespaces())
    source.close()
