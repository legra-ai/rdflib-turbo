"""Exact numeric RDFS substitutions retain RDF terms and transaction scopes."""

import decimal
import json
import sys
from pathlib import Path

import pytest
import rdflib
from owlrl import DeductiveClosure, RDFS_Semantics
from rdflib import Graph as ReferenceGraph
from rdflib import Literal, URIRef
from rdflib_turbo import Dataset, Graph, Store

FIXTURES = Path(__file__).resolve().parents[1] / "src/inference"
PREDICATE = URIRef("urn:numeric:value")


@pytest.fixture(autouse=True)
def preserve_lexical_terms(monkeypatch):
    # Compare the same exact lexical graph, without reference parser normalization.
    monkeypatch.setattr(rdflib, "NORMALIZE_LITERALS", False)


def document():
    terms = json.loads((FIXTURES / "numeric-reference-terms.json").read_text())
    return "\n".join(
        f"<urn:numeric:s{index}> <{PREDICATE}> {Literal(term['lexical'], datatype=URIRef(term['datatype']), normalize=False).n3()} ."
        for index, term in enumerate(terms)
    )


def values(graph):
    return set(graph.triples((None, PREDICATE, None)))


@pytest.mark.parametrize("api", ["dataset", "graph", "store", "transaction"])
def test_numeric_closure_matches_reference_with_untrapped_nan(api):
    source = document()
    with decimal.localcontext() as context:
        context.traps[decimal.InvalidOperation] = False
        reference = ReferenceGraph().parse(data=source, format="nt")
        DeductiveClosure(RDFS_Semantics, improved_datatypes=False).expand(reference)
        expected = values(reference)
        if api == "dataset":
            native = Dataset().parse(data=source, format="nt").infer(inference="rdfs")
            assert values(native.to_rdflib()) == expected
        elif api == "graph":
            graph = Graph(identifier=URIRef("urn:numeric:scope"))
            outside = Graph(store=graph.store, identifier=URIRef("urn:numeric:outside"))
            outside.add((URIRef("urn:outside"), PREDICATE, Literal(1)))
            graph.parse(data=source, format="nt").infer(inference="rdfs")
            assert values(graph) == expected
            assert len(outside) == 1
        else:
            with Store() as store:
                if api == "transaction":
                    with store.transaction() as transaction:
                        transaction.parse(data=source, format="nt").infer(inference="rdfs")
                        assert (
                            values(
                                ReferenceGraph().parse(
                                    data=transaction.serialize(format="ntriples"), format="nt"
                                )
                            )
                            == expected
                        )
                else:
                    store.parse(data=source, format="nt").infer(inference="rdfs")
                assert (
                    values(
                        ReferenceGraph().parse(data=store.serialize(format="ntriples"), format="nt")
                    )
                    == expected
                )


def test_signaling_nan_raises_reference_exception_and_rolls_back():
    source = '<urn:s> <urn:p> "sNaN"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n<urn:s> <urn:q> "1"^^<http://www.w3.org/2001/XMLSchema#integer> .'
    with decimal.localcontext() as context:
        context.traps[decimal.InvalidOperation] = True
        for graph in [
            ReferenceGraph().parse(data=source, format="nt"),
            Dataset().parse(data=source, format="nt"),
        ]:
            context.clear_flags()
            with pytest.raises(decimal.InvalidOperation) as error:
                if isinstance(graph, ReferenceGraph):
                    DeductiveClosure(RDFS_Semantics, improved_datatypes=False).expand(graph)
                else:
                    before = graph.serialize(format="ntriples")
                    graph.infer(inference="rdfs")
            assert error.value.args == ([decimal.InvalidOperation],)
            assert context.flags[decimal.InvalidOperation]
            if isinstance(graph, Dataset):
                assert graph.serialize(format="ntriples") == before


@pytest.mark.parametrize("right", ['"1"', '"NaN"', '"INF"', '"bad"'])
def test_decimal_float_comparison_flags_match_reference(right):
    source = f'<urn:s> <urn:p> "1.0"^^<http://www.w3.org/2001/XMLSchema#decimal> .\n<urn:s> <urn:q> {right}^^<http://www.w3.org/2001/XMLSchema#double> .'
    with decimal.localcontext() as context:
        for api in ["reference", "native"]:
            context.clear_flags()
            graph = (
                ReferenceGraph().parse(data=source, format="nt")
                if api == "reference"
                else Dataset().parse(data=source, format="nt")
            )
            if api == "reference":
                DeductiveClosure(RDFS_Semantics, improved_datatypes=False).expand(graph)
                expected = dict(context.flags)
            else:
                graph.infer(inference="rdfs")
                assert dict(context.flags) == expected


def test_numeric_conversion_snapshots_each_callers_integer_limit():
    source = (
        f'<urn:integer> <urn:p> "{"0" * 641}"^^<http://www.w3.org/2001/XMLSchema#integer> .\n'
        '<urn:boolean> <urn:p> "false"^^<http://www.w3.org/2001/XMLSchema#boolean> .'
    )
    previous = sys.get_int_max_str_digits()
    try:
        for limit, expected in [(640, False), (0, True), (640, False)]:
            sys.set_int_max_str_digits(limit)
            graph = Dataset().parse(data=source, format="nt").infer(inference="rdfs")
            assert (
                graph.query(
                    'ASK { <urn:integer> <urn:p> "false"^^<http://www.w3.org/2001/XMLSchema#boolean> }'
                ).askAnswer
                is expected
            )
    finally:
        sys.set_int_max_str_digits(previous)


def test_single_signaling_nan_uses_identity_without_comparing_its_value():
    source = '<urn:s> <urn:p> "sNaN"^^<http://www.w3.org/2001/XMLSchema#decimal> .'
    with decimal.localcontext() as context:
        context.traps[decimal.InvalidOperation] = True
        for api in ["reference", "native"]:
            context.clear_flags()
            graph = (
                ReferenceGraph().parse(data=source, format="nt")
                if api == "reference"
                else Dataset().parse(data=source, format="nt")
            )
            if api == "reference":
                DeductiveClosure(RDFS_Semantics, improved_datatypes=False).expand(graph)
            else:
                graph.infer(inference="rdfs")
            assert not context.flags[decimal.InvalidOperation]
