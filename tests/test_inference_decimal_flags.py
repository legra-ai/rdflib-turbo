"""Reference Decimal status flags are caller-visible even after failed closure."""

import decimal

import pytest
from owlrl import OWLRL_Semantics
from rdflib import Graph, Literal, URIRef
from rdflib.namespace import XSD
from rdflib_turbo import Dataset, Store

CASES = [
    ("rounded", "double", "1.234567", {"prec": 3}, {}),
    ("caught-rounding-trap", "double", "1.234567", {"prec": 3}, {"Inexact": True}),
    ("malformed-trapped", "decimal", "bad", {}, {}),
    ("malformed-untrapped", "decimal", "bad", {}, {"InvalidOperation": False}),
    ("nan", "double", "NaN", {}, {}),
    ("signaling-nan", "double", "sNaN", {}, {}),
    ("underflow", "double", "1.234e-5", {"prec": 2, "Emin": -2, "Emax": 2}, {}),
    ("overflow", "double", "9.99e3", {"prec": 2, "Emin": -2, "Emax": 2}, {"Overflow": False}),
    ("clamped", "double", "1e1", {"prec": 3, "Emin": -2, "Emax": 2, "clamp": 1}, {}),
    ("zero-clamped", "double", "0e999", {"prec": 3, "Emin": -2, "Emax": 2}, {}),
    ("integer-control", "integer", "12", {}, {}),
    ("late-failure", "double", "1.234567", {"prec": 3}, {}),
]


def configuration(context):
    return (
        context.prec,
        context.rounding,
        context.Emin,
        context.Emax,
        context.clamp,
        context.capitals,
        tuple(context.traps.items()),
    )


def execute(datatype, lexical, settings, traps, owner, late_failure):
    source = Graph()
    source.add(
        (
            URIRef("urn:flags:s"),
            URIRef("urn:flags:p"),
            Literal(lexical, datatype=XSD[datatype], normalize=False),
        )
    )
    if late_failure:
        source.parse(
            data="""@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
<urn:flags:set> a owl:AllDifferent; owl:members <urn:flags:cell> .
<urn:flags:cell> rdf:first <urn:flags:x>; rdf:rest <urn:flags:cell> .""",
            format="turtle",
        )
    if owner == "reference":
        target = OWLRL_Semantics(source, False, False)
    elif owner in {"dataset", "delta"}:
        target = Dataset.from_rdflib(source)
    else:
        target = Store().load(data=source.serialize(format="turtle"))
    with decimal.localcontext() as context:
        for key, value in settings.items():
            setattr(context, key, value)
        for key, value in traps.items():
            context.traps[getattr(decimal, key)] = value
        context.clear_flags()
        context.flags[decimal.DivisionByZero] = True
        context.flags[decimal.FloatOperation] = True
        before = configuration(context)
        error = None
        try:
            if owner == "reference":
                target.closure()
            elif owner == "delta":
                target.infer_delta()
            elif owner == "transaction":
                with target.transaction() as transaction:
                    transaction.infer()
            else:
                target.infer()
        except Exception as failure:
            error = (type(failure), failure.args)
        assert decimal.getcontext() is context
        assert configuration(context) == before
        return {signal.__name__: bool(value) for signal, value in context.flags.items()}, error


@pytest.mark.parametrize("owner", ["dataset", "delta", "store", "transaction"])
@pytest.mark.parametrize("name,datatype,lexical,settings,traps", CASES)
def test_caller_decimal_flags_match_original_after_closure(
    name, datatype, lexical, settings, traps, owner
):
    expected_flags, expected_error = execute(
        datatype, lexical, settings, traps, "reference", name == "late-failure"
    )
    actual_flags, actual_error = execute(
        datatype, lexical, settings, traps, owner, name == "late-failure"
    )
    assert actual_error == expected_error
    assert actual_flags == expected_flags
