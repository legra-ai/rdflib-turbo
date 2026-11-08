"""Restricted standard numeric closure over installed native owners."""

import decimal

import pytest
from rdflib import Literal, Namespace
from rdflib.namespace import OWL, RDF, RDFS, XSD
from rdflib_turbo import Graph, Store, StoreDataset

EX = Namespace("urn:standard-numeric:")
MODE = "owl2-rl-numeric-core"


def literal(text, datatype):
    return Literal(text, datatype=datatype, normalize=False)


@pytest.mark.parametrize("union", [False, True])
def test_numeric_mode_exact_values_generalized_heads_and_scope(union):
    owner = StoreDataset() if union else Graph()
    source = owner.graph(EX.source) if union else owner
    integer = literal("01", XSD.integer)
    decimal_value = literal("1.0", XSD.decimal)
    binary = literal("1.0", XSD.float)
    for index, value in enumerate((integer, decimal_value, binary)):
        source.add((EX[f"s{index}"], EX.p, value))
    destination = owner.graph(EX.output) if union else None
    owner.infer(inference=MODE, destination=destination)
    result = destination if union else owner
    assert (integer, OWL.sameAs, decimal_value) in result
    assert (decimal_value, OWL.sameAs, integer) in result
    assert (integer, OWL.differentFrom, binary) in result
    assert (integer, RDF.type, XSD.unsignedByte) in result
    assert (integer, RDF.type, OWL.real) not in result
    assert (EX.s0, RDF.type, RDFS.Resource) not in result
    before = len(result)
    owner.infer(inference=MODE, destination=destination)
    assert len(result) == before
    if union:
        assert len(source) == 3
        assert (EX.s0, EX.p, integer) not in destination


def test_numeric_nan_zero_and_type_violation():
    graph = Graph()
    nan_a = literal("NaN", XSD.double)
    nan_b = literal(" NaN ", XSD.double)
    zero = literal("0", XSD.float)
    negative_zero = literal("-0", XSD.float)
    for index, value in enumerate((nan_a, nan_b, zero, negative_zero)):
        graph.add((EX[f"s{index}"], EX.p, value))
    graph.add((zero, RDF.type, XSD.integer))
    graph.infer(inference=MODE)
    assert (nan_a, OWL.sameAs, nan_b) in graph
    assert (zero, OWL.differentFrom, negative_zero) in graph
    assert len(graph.last_inference.error_messages) == 1
    assert "numeric/dt-not-type" in graph.last_inference.error_messages[0]


@pytest.mark.parametrize("datatype,text", [(XSD.unsignedByte, "256"), (XSD.double, "+NaN")])
def test_numeric_lexical_failure_is_atomic(datatype, text):
    graph = Graph()
    graph.add((EX.valid, EX.p, literal("1", XSD.integer)))
    graph.add((EX.bad, EX.p, literal(text, datatype)))
    before = set(graph)
    with pytest.raises((ValueError, RuntimeError, NotImplementedError), match="numeric"):
        graph.infer(inference=MODE)
    assert set(graph) == before
    assert graph.last_inference is None


def test_numeric_transaction_failure_aborts_preceding_writes():
    graph = Graph()
    with pytest.raises((ValueError, RuntimeError, NotImplementedError), match="numeric"):
        with graph.transaction() as transaction:
            transaction.add((EX.pending, EX.p, literal("bad", XSD.integer)))
            transaction.infer(inference=MODE)
    assert len(graph) == 0


@pytest.mark.parametrize("option", ["axiomatic_triples", "datatype_axioms"])
def test_numeric_mode_rejects_unimplemented_axiom_options(option):
    graph = Graph()
    with pytest.raises((ValueError, RuntimeError, NotImplementedError), match="numeric"):
        graph.infer(inference=MODE, **{option: True})
    assert len(graph) == 0


def test_numeric_mode_ignores_python_decimal_precision_and_flags():
    graph = Graph()
    graph.add((EX.s, EX.p, literal("123456789012345678901234567890", XSD.integer)))
    with decimal.localcontext() as context:
        context.prec = 1
        context.clear_flags()
        flags = context.flags.copy()
        graph.infer(inference=MODE)
        assert context.flags == flags


def test_raw_store_numeric_mode_and_empty_domain():
    store = Store()
    store.infer(inference=MODE)
    assert len(store) == 0
