"""Check native numeric fixtures against isolated, pinned RDFLib/OWL-RL values."""

import os
import subprocess
import sys
from pathlib import Path

FIXTURES = Path(__file__).resolve().parents[1] / "src/inference"
ORACLE = r"""
import decimal
import json
import logging
import sys
import warnings
from importlib.metadata import version
from pathlib import Path
from owlrl.RDFSClosure import RDFS_Semantics
from rdflib import Literal, URIRef

assert version("rdflib") == "7.6.0"
assert version("owlrl") == "7.6.2"
assert sys.get_int_max_str_digits() == 4300
logging.disable(logging.CRITICAL)
warnings.simplefilter("ignore")
root = Path(sys.argv[1])
terms = json.loads((root / "numeric-reference-terms.json").read_text())
count = 0
for name in sorted(root.glob("numeric-reference-pairs-*.jsonl")):
    for line in name.read_text().splitlines():
        row = json.loads(line)
        with decimal.localcontext() as context:
            context.traps[decimal.InvalidOperation] = row["invalid_trap"]
            specs = [terms[row[side]] for side in ("left", "right")]
            literals = [Literal(term["lexical"], datatype=URIRef(term["datatype"]), normalize=False) for term in specs]
            tags = []
            for term, literal in zip(specs, literals):
                if term["datatype"].endswith("#string"):
                    tags.append("not_numeric")
                elif literal.value is None:
                    tags.append("missing")
                else:
                    tags.append("numeric")
            args = None
            try:
                reference = str(bool(RDFS_Semantics._literals_same_as(*literals))).lower()
            except decimal.InvalidOperation as error:
                reference, args = "InvalidComparison", repr(error.args)
                assert error.args == ([decimal.InvalidOperation],)
            expected = "not_numeric" if "not_numeric" in tags else "missing" if "missing" in tags else reference
            assert (expected, reference, args) == (row["expected"], row["reference"], row["error_args"]), (name.name, row, expected, reference, args)
            count += 1
assert count == 1352
xsd = "http://www.w3.org/2001/XMLSchema#"
assert Literal("\u0661_\u0662", datatype=URIRef(xsd + "integer"), normalize=False).value == 12
assert Literal("1_\u0662.0", datatype=URIRef(xsd + "double"), normalize=False).value == 12
assert Literal("0" * 4301, datatype=URIRef(xsd + "integer"), normalize=False).value is None
sys.set_int_max_str_digits(0)
assert Literal("0" * 4301, datatype=URIRef(xsd + "integer"), normalize=False).value == 0
print("Verified", count, "numeric reference pairs and Unicode/digit-limit edges")
"""


def test_numeric_fixtures_match_actual_pinned_reference():
    environment = dict(os.environ, PYTHONINTMAXSTRDIGITS="4300")
    completed = subprocess.run(
        [sys.executable, "-c", ORACLE, str(FIXTURES)],
        env=environment,
        text=True,
        capture_output=True,
        check=False,
    )
    assert completed.returncode == 0, completed.stderr
    assert "1352 numeric reference pairs" in completed.stdout
