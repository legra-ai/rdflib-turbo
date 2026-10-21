"""Retain actual pinned converter outcomes independently of the Rust parser."""

import os
import subprocess
import sys
from pathlib import Path

FIXTURES = Path(__file__).resolve().parents[1] / "src/inference/diagnostic_datatypes"
ORACLE = r"""
import decimal
import json
import logging
import sys
import warnings
from importlib.metadata import version
from pathlib import Path
from owlrl.DatatypeHandling import AltXSDToPYTHON
from rdflib import Literal, URIRef

assert version("owlrl") == "7.6.2", "Review pinned lexical fixtures when updating owlrl"
assert sys.get_int_max_str_digits() == 4300
# Intentionally invalid reference literals log conversion failures in this child.
logging.disable(logging.CRITICAL)
warnings.simplefilter("ignore")
signals = ["InvalidOperation", "Inexact", "Rounded", "Subnormal", "Underflow", "Overflow", "Clamped"]
count = 0
for name in ["reference.jsonl", "float-contexts.jsonl", "double-contexts.jsonl", "decimal-contexts.jsonl", "cardinality-zero.jsonl"]:
    for line in (Path(sys.argv[1]) / name).read_text().splitlines():
        row = json.loads(line)
        with decimal.localcontext() as context:
            if "precision" in row:
                context.prec = row["precision"]
                context.rounding = row["rounding"]
                context.Emin, context.Emax = row["emin"], row["emax"]
                context.clamp = row["clamp"]
                for signal, enabled in zip(signals, row["traps"]):
                    context.traps[getattr(decimal, signal)] = enabled
            if "invalid_trap" in row:
                context.traps[decimal.InvalidOperation] = row["invalid_trap"]
            error = None
            try:
                if name == "cardinality-zero.jsonl":
                    value = Literal(row["lexical"], datatype=URIRef(row["datatype"]), normalize=False).value
                    result = str(value == 0).lower()
                else:
                    datatype = row.get("datatype", "http://www.w3.org/2001/XMLSchema#" + row.get("kind", ""))
                    AltXSDToPYTHON[URIRef(datatype)](row["lexical"])
                    result = "valid"
            except ValueError as exception:
                result, error = "diagnostic", exception
            except Exception as exception:
                result = "InvalidComparison" if name == "cardinality-zero.jsonl" else type(exception).__name__
                error = exception
            assert result == row["expected"], (name, count, row, result)
            if result in {"InvalidOperation", "InvalidComparison"}:
                signal = decimal.InvalidOperation if result == "InvalidComparison" else decimal.ConversionSyntax
                assert error.args == ([signal],)
            if result == "IndexError":
                assert error.args == ("string index out of range",)
            count += 1
print("Verified", count, "pinned reference outcomes")
"""


def test_native_lexical_fixtures_match_actual_pinned_reference():
    environment = dict(os.environ, PYTHONINTMAXSTRDIGITS="4300")
    completed = subprocess.run(
        [sys.executable, "-c", ORACLE, str(FIXTURES)],
        env=environment,
        text=True,
        capture_output=True,
        check=False,
    )
    assert completed.returncode == 0, completed.stderr
    assert "pinned reference outcomes" in completed.stdout
