"""Borrow native graph scopes without reading or serializing Python triples."""

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib_turbo import Graph, StoreDataset
from rdflib_turbo._native import SharedShapes
from rdflib_turbo._native import shacl as _native_shacl

SHAPES = """@prefix sh: <http://www.w3.org/ns/shacl#>.
<urn:S> sh:targetNode <urn:s>;
 sh:rule [a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:derived>; sh:object true];
 sh:property [sh:path <urn:p>; sh:minCount 1].
"""
P = URIRef("urn:p")
S = URIRef("urn:s")
DERIVED = (S, URIRef("urn:derived"), Literal(True))


@pytest.mark.parametrize("identifier", [None, URIRef("urn:selected"), BNode("selected")])
@pytest.mark.parametrize("inplace", [False, True])
def test_validation_is_scoped_and_rule_output_returns_to_selected_graph(identifier, inplace):
    dataset = StoreDataset()
    graph = dataset.default_graph if identifier is None else dataset.graph(identifier)
    unrelated = dataset.graph(URIRef("urn:unrelated"))
    unrelated.add((S, P, Literal(1)))
    program = SharedShapes(
        SHAPES,
        True,
        [],
        _policy=_native_shacl._REGEX_POLICY,
        _comparison_policy=_native_shacl._VALUE_COMPARISON_POLICY,
    )
    with graph._validate_shared_shapes(program, "{}", inplace) as result:
        assert not result[0], "unrelated context must not satisfy the selected Graph"
    assert (DERIVED in graph) is inplace
    assert DERIVED not in unrelated
    graph.add((S, P, Literal(2)))
    with graph._validate_shared_shapes(program, "{}", False) as result:
        assert result[0]


@pytest.mark.parametrize("default_union", [False, True])
def test_dataset_validation_reads_union_and_writes_only_default_graph(default_union):
    dataset = StoreDataset(default_union=default_union)
    named = dataset.graph(URIRef("urn:data"))
    named.add((S, P, Literal(1)))
    with dataset._validate_shared_shapes(
        SharedShapes(
            SHAPES,
            True,
            [],
            _policy=_native_shacl._REGEX_POLICY,
            _comparison_policy=_native_shacl._VALUE_COMPARISON_POLICY,
        ),
        "{}",
        True,
    ) as result:
        assert result[0]
    assert DERIVED in dataset.default_graph
    assert DERIVED not in named


def test_pending_graph_validation_acceptance_and_report_errors_are_atomic():
    graph = Graph(identifier=URIRef("urn:selected"))
    program = SharedShapes(
        SHAPES,
        True,
        [],
        _policy=_native_shacl._REGEX_POLICY,
        _comparison_policy=_native_shacl._VALUE_COMPARISON_POLICY,
    )
    with pytest.raises(ValueError, match="report decoding"):
        with graph.transaction() as transaction:
            transaction.add((S, P, Literal(1)))
            with transaction._validate_shared_shapes(program, "{}", True) as result:
                assert result[0]
                assert DERIVED not in transaction
                raise ValueError("report decoding")
    assert len(graph) == 0
    with graph.transaction() as transaction:
        transaction.add((S, P, Literal(1)))
        with transaction._validate_shared_shapes(program, "{}", True) as result:
            assert result[0]
        assert DERIVED in transaction
    assert DERIVED in graph


def test_namespace_mutation_while_accepting_report_aborts_pending_writes():
    graph = Graph()
    with pytest.raises(RuntimeError, match="namespace"):
        with graph.transaction() as transaction:
            transaction.add((S, P, Literal(1)))
            with transaction._validate_shared_shapes(
                SharedShapes(
                    SHAPES,
                    True,
                    [],
                    _policy=_native_shacl._REGEX_POLICY,
                    _comparison_policy=_native_shacl._VALUE_COMPARISON_POLICY,
                ),
                "{}",
                True,
            ):
                transaction.bind("changed", URIRef("urn:changed:"))
    assert len(graph) == 0


def test_original_graph_namespace_mutation_aborts_implicit_validation():
    graph = Graph()
    graph.add((S, P, Literal(1)))
    with pytest.raises(RuntimeError, match="namespace"):
        with graph._validate_shared_shapes(
            SharedShapes(
                SHAPES,
                True,
                [],
                _policy=_native_shacl._REGEX_POLICY,
                _comparison_policy=_native_shacl._VALUE_COMPARISON_POLICY,
            ),
            "{}",
            True,
        ):
            graph.bind("changed", URIRef("urn:changed:"))
    assert DERIVED not in graph
    assert len(graph) == 1


def test_invalid_settings_abort_pending_graph_changes_even_when_caught():
    graph = Graph()
    with pytest.raises(RuntimeError, match=r"aborted|active|closed|failed"):
        with graph.transaction() as transaction:
            transaction.add((S, P, Literal(1)))
            with pytest.raises(ValueError):
                with transaction._validate_shared_shapes(
                    SharedShapes(
                        SHAPES,
                        True,
                        [],
                        _policy=_native_shacl._REGEX_POLICY,
                        _comparison_policy=_native_shacl._VALUE_COMPARISON_POLICY,
                    ),
                    "[]",
                    True,
                ):
                    pytest.fail("invalid settings must fail before report handling")
    assert len(graph) == 0


def test_rdf_mutation_during_graph_report_handling_aborts():
    graph = Graph()
    with pytest.raises(RuntimeError, match="changed"):
        with graph.transaction() as transaction:
            transaction.add((S, P, Literal(1)))
            with transaction._validate_shared_shapes(
                SharedShapes(
                    SHAPES,
                    True,
                    [],
                    _policy=_native_shacl._REGEX_POLICY,
                    _comparison_policy=_native_shacl._VALUE_COMPARISON_POLICY,
                ),
                "{}",
                True,
            ):
                transaction.add((S, P, Literal(1)))
    assert len(graph) == 0


def test_committed_scoped_validation_reads_snapshot_while_writer_is_open():
    graph = Graph()
    graph.add((S, P, Literal(1)))
    with graph.transaction() as transaction:
        transaction.remove((S, P, None))
        with graph._validate_shared_shapes(
            SharedShapes(
                SHAPES,
                True,
                [],
                _policy=_native_shacl._REGEX_POLICY,
                _comparison_policy=_native_shacl._VALUE_COMPARISON_POLICY,
            ),
            "{}",
            False,
        ) as result:
            assert result[0]
        with transaction._validate_shared_shapes(
            SharedShapes(
                SHAPES,
                True,
                [],
                _policy=_native_shacl._REGEX_POLICY,
                _comparison_policy=_native_shacl._VALUE_COMPARISON_POLICY,
            ),
            "{}",
            False,
        ) as result:
            assert not result[0]


@pytest.mark.parametrize("dataset", [False, True])
@pytest.mark.parametrize("inplace", [False, True])
def test_public_compiled_validation_does_not_materialize_native_input(
    monkeypatch, dataset, inplace
):
    import pyshacl_turbo
    from rdflib import Graph as ReferenceGraph

    compiled = pyshacl_turbo.CompiledShapes(ReferenceGraph().parse(data=SHAPES), advanced=True)
    source = StoreDataset() if dataset else Graph(identifier=URIRef("urn:chosen"))
    graph = source.default_graph if dataset else source
    graph.add((S, P, Literal(1)))

    def materialized(*args, **kwargs):
        pytest.fail("native validation must not materialize or serialize source RDF")

    monkeypatch.setattr(pyshacl_turbo, "_input", materialized)
    monkeypatch.setattr(source, "serialize", materialized)
    conforms, report, text = compiled.validate(source, inplace=inplace)
    assert conforms
    assert report
    assert "Conforms: True" in text
    assert (DERIVED in graph) is inplace
