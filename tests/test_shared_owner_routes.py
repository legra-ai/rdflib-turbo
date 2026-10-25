"""Exercise real shared-owner rules, shape compilation and bounded read lifetimes."""

import pytest
from rdflib import Graph as ReferenceGraph
from rdflib import Literal, URIRef
from rdflib_turbo import Graph, Store, StoreDataset
from rdflib_turbo._native import SharedShapes
from rdflib_turbo._native import shacl as native_shacl
from rdflib_turbo._reads import context_names, physical_rows, union_rows

from .inference_observers import physical_keys

SHAPES = """@prefix sh: <http://www.w3.org/ns/shacl#>.
<urn:Shape> sh:targetNode <urn:s>; sh:rule [a sh:TripleRule;
 sh:subject sh:this; sh:predicate <urn:derived>; sh:object true];
 sh:property [sh:path <urn:p>; sh:minCount 1]."""
SOURCE = '<urn:s> <urn:p> "source" .'
DERIVED = (URIRef("urn:s"), URIRef("urn:derived"), Literal(True))


def program():
    return SharedShapes(SHAPES, True, [], _policy=native_shacl._REGEX_POLICY)


@pytest.mark.parametrize("dataset", [False, True])
@pytest.mark.parametrize("inplace", [False, True])
def test_graph_rule_result_retains_scope_and_source_ownership(dataset, inplace):
    source = StoreDataset() if dataset else Graph(identifier=URIRef("urn:chosen"))
    source.parse(data=SOURCE, format="nt")
    before = physical_keys(source.store.backend)
    with source._expand_shared_shapes(program(), "{}", inplace) as (result, warnings):
        assert warnings == []
        assert (result is source) is inplace
        graph = result.default_graph if dataset else result
    assert DERIVED in graph
    assert (
        physical_keys(source.store.backend) != before
        if inplace
        else physical_keys(source.store.backend) == before
    )
    source.close()
    if not inplace:
        assert DERIVED in graph, "the returned owner must outlive the original"


@pytest.mark.parametrize("explicit", [False, True])
def test_rule_consumer_failure_aborts_pending_and_derived_graph_writes(explicit):
    source = Graph().parse(data=SOURCE, format="nt")
    before = physical_keys(source.store.backend)
    with pytest.raises(ValueError, match="consumer rejected output"):
        if explicit:
            with source.transaction() as transaction:
                transaction.add((URIRef("urn:pending"), URIRef("urn:p"), Literal(1)))
                with transaction._expand_shared_shapes(program(), "{}", True):
                    raise ValueError("consumer rejected output")
        else:
            with source._expand_shared_shapes(program(), "{}", True):
                raise ValueError("consumer rejected output")
    assert physical_keys(source.store.backend) == before


@pytest.mark.parametrize("inplace", [False, True])
def test_store_rule_result_owns_committed_changes_and_copy_is_independent(inplace):
    with Store() as source:
        source.parse(data=SOURCE, format="nt")
        before = physical_keys(source)
        with source._expand_shared_shapes(program(), "{}", inplace) as (result, warnings):
            assert warnings == []
            assert (result is source) is inplace
        assert physical_keys(result) > before
        assert physical_keys(source) > before if inplace else physical_keys(source) == before


@pytest.mark.parametrize("explicit", [False, True])
def test_store_rule_consumer_failure_aborts_all_pending_writes(explicit):
    with Store() as source:
        source.parse(data=SOURCE, format="nt")
        before = physical_keys(source)
        with pytest.raises(ValueError, match="consumer rejected output"):
            if explicit:
                with source.transaction() as transaction:
                    transaction.parse(data='<urn:pending> <urn:p> "pending" .', format="nt")
                    with transaction._expand_shared_shapes(program(), "{}", True):
                        raise ValueError("consumer rejected output")
            else:
                with source._expand_shared_shapes(program(), "{}", True):
                    raise ValueError("consumer rejected output")
        assert physical_keys(source) == before


@pytest.mark.parametrize("dataset", [False, True])
def test_shapes_compile_from_native_scopes_and_validate_another_owner(dataset):
    source = StoreDataset() if dataset else Graph(identifier=URIRef("urn:shapes"))
    source.bind("ex", "urn:")
    source.parse(data=SHAPES, format="turtle")
    native, namespaces = source._compile_shared_shapes(True)
    compiled = SharedShapes.from_compiled(native)
    assert ("ex", "urn:") in namespaces
    data = Graph().parse(data=SOURCE, format="nt")
    with data._validate_shared_shapes(compiled, "{}", False) as result:
        assert result[0]
    data.remove((URIRef("urn:s"), URIRef("urn:p"), None))
    with data._validate_shared_shapes(compiled, "{}", False) as result:
        assert not result[0]
    with source.transaction() as transaction:
        with pytest.raises(NotImplementedError, match="pending transactions"):
            transaction._compile_shared_shapes(True)


def test_namespace_change_during_compile_fails_instead_of_returning_stale_program(monkeypatch):
    source = Graph().parse(data=SHAPES, format="turtle")
    namespaces = source.store._namespaces
    original = namespaces.items

    def changed_items():
        captured = list(original())
        source.bind("changed", "urn:changed:")
        return captured

    monkeypatch.setattr(namespaces, "items", changed_items)
    with pytest.raises(RuntimeError, match="changed while compiling"):
        source._compile_shared_shapes(True)


def test_delta_capture_rejects_pending_owners_without_committing_them():
    source = StoreDataset()
    with source.transaction() as transaction:
        transaction.add((URIRef("urn:pending"), URIRef("urn:p"), Literal(1)))
        with pytest.raises(NotImplementedError, match="committed memory"):
            transaction._capture_changes()
        with pytest.raises(NotImplementedError, match="committed memory"):
            transaction.infer_delta()
    assert len(source) == 1


@pytest.mark.parametrize("reject_namespace", [False, True])
def test_inference_capture_publishes_diagnostics_or_closes_failed_consumer_lease(
    monkeypatch, reject_namespace
):
    source = StoreDataset().parse(
        data="<urn:a> <http://www.w3.org/2002/07/owl#sameAs> <urn:b> .\n<urn:a> <http://www.w3.org/2002/07/owl#differentFrom> <urn:b> .",
        format="nt",
    )
    if reject_namespace:
        original = ReferenceGraph.bind

        def bind(graph, prefix, *args, **kwargs):
            if prefix == "err":
                raise ValueError("diagnostic namespace consumer failed")
            return original(graph, prefix, *args, **kwargs)

        monkeypatch.setattr(ReferenceGraph, "bind", bind)
        with pytest.raises(ValueError, match="diagnostic namespace consumer failed"):
            source._capture_changes()
    else:
        changes = source._capture_changes()
        try:
            assert changes.outcome.error_messages
            assert dict(source.namespaces())["err"] == URIRef(
                "http://www.daml.org/2002/03/agents/agent-ont#"
            )
        finally:
            changes.close()
    source.add((URIRef("urn:after"), URIRef("urn:p"), Literal("lease released")))
    assert (URIRef("urn:after"), URIRef("urn:p"), Literal("lease released")) in source


def test_store_document_inputs_are_atomic_and_accept_paths_and_utf8(tmp_path):
    document = tmp_path / "source.nt"
    document.write_text(SOURCE, encoding="utf-8")
    with Store() as source:
        assert source.parse(document, format="nt") is source
        assert source.parse(data=b'<urn:bytes> <urn:p> "value" .', format="nt") is source
        before = physical_keys(source)
        for arguments in ({}, {"source": document, "data": SOURCE}, {"data": 42}):
            with pytest.raises(TypeError):
                source.parse(**arguments)
            assert physical_keys(source) == before


def test_partial_physical_and_union_scans_release_real_native_cursors():
    source = StoreDataset()
    common = (URIRef("urn:common"), URIRef("urn:p"), Literal("shared"))
    for index in range(400):
        graph = source.graph(URIRef(f"urn:g{index}"))
        graph.add((URIRef(f"urn:s{index}"), URIRef("urn:p"), Literal(index)))
        graph.add(common)
    pattern = (None, None, None)
    for context, count in ((source.default_graph, 0), (source.graph(URIRef("urn:g1")), 2)):
        assert len(list(physical_rows(source.store, pattern, context))) == count
    rows = physical_rows(source.store, pattern, None)
    assert next(rows)
    rows.close()
    rows = union_rows(source.store, pattern)
    triple, contexts = next(rows)
    assert triple
    contexts.close()
    rows.close()
    rows = union_rows(source.store, common)
    triple, contexts = next(rows)
    assert triple == common
    names = list(context_names(source.store, contexts))
    assert set(names) == {URIRef(f"urn:g{index}") for index in range(400)}
    rows.close()
    source.add((URIRef("urn:after"), URIRef("urn:p"), Literal("released")))
    assert len(source) == 402


@pytest.mark.parametrize("public_id", [None, "https://example.test/base/"])
def test_store_file_input_resolves_relative_iris_against_the_selected_public_base(
    tmp_path, public_id
):
    document = tmp_path / "relative.ttl"
    document.write_text('<relative> <urn:p> "value" .', encoding="utf-8")
    expected = URIRef(
        "https://example.test/base/relative" if public_id else (tmp_path / "relative").as_uri()
    )
    with Store() as source:
        source.parse(document, format="turtle", publicID=public_id)
        assert physical_keys(source) == {
            (
                ("URIRef", str(expected), None, None),
                ("URIRef", "urn:p", None, None),
                ("Literal", "value", None, None),
                None,
            )
        }
