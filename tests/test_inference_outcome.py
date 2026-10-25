"""Successful diagnostic records and RDF publication share one commit boundary."""

import decimal

import pytest
from owlrl import DeductiveClosure, OWLRL_Semantics
from rdflib import OWL, RDF, XSD, Literal, Namespace, URIRef
from rdflib import Graph as ReferenceGraph
from rdflib.compare import isomorphic
from rdflib_turbo import Dataset, Store

ERR = Namespace("http://www.daml.org/2002/03/agents/agent-ont#")
CONFLICT = """@prefix owl: <http://www.w3.org/2002/07/owl#> .
<urn:x> a owl:Nothing ."""


def last_outcome(source):
    # Dataset exposes the facade property; Store/Transaction ABI tests retain
    # their backend invocation record rather than recreating a legacy wrapper.
    return source.last_inference if isinstance(source, Dataset) else source._last_inference


def ordinary(graph):
    if isinstance(graph, Dataset):
        result = ReferenceGraph()
        for subject, predicate, obj, _ in graph.to_rdflib().quads():
            result.add((subject, predicate, obj))
        return result
    return ReferenceGraph().parse(data=graph.serialize(format="ntriples"), format="nt")


def test_outcome_is_immutable_and_prior_history_is_republished_without_reasoning():
    graph = Dataset().parse(data=CONFLICT)
    prior = ["earlier", "earlier", "another"]
    graph.infer(prior_error_messages=prior)
    outcome = last_outcome(graph)
    assert outcome.error_messages[:3] == prior
    assert outcome.error_messages[3:] == ["urn:x is defined of type 'Nothing'"]
    outcome.error_messages.clear()
    assert len(outcome.error_messages) == 4
    with pytest.raises(AttributeError):
        outcome.error_messages = []
    assert prior == ["earlier", "earlier", "another"]
    actual = ordinary(graph)
    nodes = set(actual.subjects(RDF.type, ERR.ErrorMessage))
    assert len(nodes) == 4
    assert all(set(actual.predicates(node)) == {RDF.type, ERR.error} for node in nodes)
    # A later invocation republishes retained messages even with no contradiction.
    clean = Dataset().infer(inference="rdfs", prior_error_messages=outcome.error_messages)
    assert last_outcome(clean).error_messages == outcome.error_messages
    assert len(set(ordinary(clean).subjects(RDF.type, ERR.ErrorMessage))) == 4


def test_delta_contains_diagnostics_and_shares_invocation_record_with_source():
    graph = Dataset().parse(data=CONFLICT)
    before = set(ordinary(graph))
    delta = graph.infer_delta(destination="urn:diagnostics")
    assert last_outcome(graph) is last_outcome(delta)
    actual, added = ordinary(graph), ordinary(delta)
    assert set(actual) == before | set(added)
    assert not before.intersection(added)
    assert len(set(added.subjects(RDF.type, ERR.ErrorMessage))) == 1
    assert dict(graph.namespaces())["err"] == URIRef(ERR)
    assert dict(delta.namespaces())["err"] == URIRef(ERR)
    quads = list(delta.to_rdflib().quads())
    assert quads and all(str(quad[3]) == "urn:diagnostics" for quad in quads)


@pytest.mark.parametrize("delta", [False, True])
def test_failed_invocation_clears_only_current_wrapper_record(delta):
    graph = Dataset().parse(data=CONFLICT).infer()
    historical = last_outcome(graph)
    before = set(ordinary(graph))
    operation = graph.infer_delta if delta else graph.infer
    with pytest.raises(ValueError):
        operation(inference="invalid")
    assert last_outcome(graph) is None
    assert historical.error_messages == ["urn:x is defined of type 'Nothing'"]
    assert set(ordinary(graph)) == before


@pytest.mark.parametrize(
    "datatype,value,exception,signal",
    [
        ("decimal", "bad", decimal.InvalidOperation, decimal.ConversionSyntax),
        ("dateTime", "", IndexError, None),
    ],
)
@pytest.mark.parametrize("api", ["dataset", "store", "transaction"])
def test_converter_failure_preserves_payload_and_rolls_back_inference(
    datatype, value, exception, signal, api
):
    document = f'<urn:x> <urn:p> "{value}"^^<http://www.w3.org/2001/XMLSchema#{datatype}> .'
    graph = Dataset().parse(data=document) if api == "dataset" else Store().parse(data=document)
    before = set(ordinary(graph))
    prior = ["retained"]
    try:
        if api == "transaction":
            with pytest.raises(RuntimeError, match=r"closed|active|abort|transaction"):
                with graph.transaction() as transaction:
                    transaction.parse(data="<urn:pending> <urn:p> <urn:o> .")
                    with pytest.raises(exception) as failure:
                        transaction.infer(prior_error_messages=prior)
                    assert last_outcome(transaction) is None
        else:
            with pytest.raises(exception) as failure:
                graph.infer(prior_error_messages=prior)
        expected_args = ([signal],) if signal else ("string index out of range",)
        assert failure.value.args == expected_args
        assert last_outcome(graph) is None
        assert prior == ["retained"]
        assert set(ordinary(graph)) == before
    finally:
        if api != "dataset":
            graph.close()


def test_provisional_outcome_does_not_publish_into_base_owner_after_rollback():
    with Store() as store:
        with pytest.raises(LookupError):
            with store.transaction() as transaction:
                transaction.parse(data=CONFLICT).infer()
                outcome = last_outcome(transaction)
                assert outcome.error_messages == ["urn:x is defined of type 'Nothing'"]
                assert len(ordinary(transaction)) > 0
                raise LookupError("abort caller scope")
        assert len(store) == 0
        assert last_outcome(store) is None
        assert outcome.error_messages == ["urn:x is defined of type 'Nothing'"]


def test_namespace_collision_preserves_existing_binding():
    graph = Dataset().parse(data="@prefix err: <urn:already-bound:> . " + CONFLICT).infer()
    bindings = dict(graph.namespaces())
    assert bindings["err"] == URIRef("urn:already-bound:")
    assert bindings["err1"] == URIRef(ERR)


@pytest.mark.parametrize("fault", ["context", "request"])
def test_caught_boundary_failure_aborts_transaction_and_releases_writer(monkeypatch, fault):
    with Store() as store:
        original = LookupError("caller conversion configuration failed")
        with pytest.raises(RuntimeError, match=r"closed|active|abort|transaction"):
            with store.transaction() as transaction:
                transaction.parse(data="<urn:pending> <urn:p> <urn:o> .")
                if fault == "context":

                    def fail():
                        raise original

                    monkeypatch.setattr(decimal, "getcontext", fail)
                    with pytest.raises(LookupError) as failure:
                        transaction.infer()
                    assert failure.value is original
                else:
                    with pytest.raises(TypeError):
                        transaction.infer(prior_error_messages=[object()])
                assert last_outcome(transaction) is None
        assert len(store) == 0
        monkeypatch.undo()
        with store.transaction() as transaction:
            transaction.parse(data="<urn:committed> <urn:p> <urn:o> .")
        assert len(store) == 1


@pytest.mark.parametrize("api", ["dataset", "store"])
def test_recursive_diagnostic_list_is_execution_failure_with_no_publication(api):
    document = """@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
<urn:set> a owl:AllDifferent; owl:members <urn:cell> .
<urn:cell> rdf:first <urn:x>; rdf:rest <urn:cell> ."""
    graph = Dataset().parse(data=document) if api == "dataset" else Store().parse(data=document)
    before = set(ordinary(graph))
    expected = ReferenceGraph().parse(data=document, format="turtle")
    with pytest.raises(ValueError, match="List contains a recursive rdf:rest reference"):
        DeductiveClosure(OWLRL_Semantics).expand(expected)
    try:
        with pytest.raises(ValueError, match="List contains a recursive rdf:rest reference"):
            graph.infer(prior_error_messages=["must not publish"])
        assert last_outcome(graph) is None
        assert set(ordinary(graph)) == before
        if api == "dataset":
            assert "err" not in dict(graph.namespaces())
    finally:
        if api == "store":
            graph.close()


def test_signaling_nan_cardinality_preserves_invalid_comparison_exception():
    reference = ReferenceGraph()
    restriction, subject, predicate, obj = map(URIRef, ["urn:R", "urn:x", "urn:p", "urn:y"])
    reference.add((restriction, OWL.onProperty, predicate))
    reference.add(
        (restriction, OWL.maxCardinality, Literal("sNaN", datatype=XSD.decimal, normalize=False))
    )
    reference.add((subject, RDF.type, restriction))
    reference.add((subject, predicate, obj))
    graph = Dataset.from_rdflib(reference)
    before = set(ordinary(graph))
    with decimal.localcontext() as context:
        context.traps[decimal.InvalidOperation] = True
        with pytest.raises(decimal.InvalidOperation) as expected:
            DeductiveClosure(OWLRL_Semantics).expand(reference)
        with pytest.raises(decimal.InvalidOperation) as actual:
            graph.infer()
    assert expected.value.args == actual.value.args == ([decimal.InvalidOperation],)
    assert last_outcome(graph) is None
    assert set(ordinary(graph)) == before


def test_repeated_closure_reasons_over_old_nodes_and_appends_fresh_diagnostics():
    expected = ReferenceGraph().parse(data=CONFLICT, format="turtle")
    semantics = OWLRL_Semantics(expected, False, False)
    graph = Dataset().parse(data=CONFLICT)
    for count in [1, 2]:
        semantics.closure()
        prior = None if last_outcome(graph) is None else last_outcome(graph).error_messages
        graph.infer(prior_error_messages=prior)
        assert last_outcome(graph).error_messages == semantics.error_messages
        actual = ordinary(graph)
        assert isomorphic(actual, expected)
        if count == 2:
            assert any(isinstance(subject, Literal) for subject in actual.subjects())
        assert len(set(actual.subjects(RDF.type, ERR.ErrorMessage))) == count
