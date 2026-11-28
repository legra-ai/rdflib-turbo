"""W3C RDFC-1.0 conformance ratchet for rdflib-turbo, graded beside RDFLib's equivalent.

The ``rdfc10`` section of the committed baseline pins exact pass counts and
ledgers every turbo failure. Rewrite it with
``python scripts/rdfc_conformance.py --write`` and review the change.
"""

import importlib.util
import sys
import time
from pathlib import Path
from types import ModuleType

import pytest

ROOT = Path(__file__).resolve().parents[1]


def load_harness() -> ModuleType:
    spec = importlib.util.spec_from_file_location(
        "rdfc_conformance", ROOT / "scripts" / "rdfc_conformance.py"
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    # Dataclass annotations resolve through sys.modules.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


harness = load_harness()
UPDATE = "rewrite the rdfc10 baseline with `python scripts/rdfc_conformance.py --write`"


@pytest.fixture(scope="module")
def report():
    return harness.Report.run()


@pytest.fixture(scope="module")
def baseline():
    return harness.Baseline.load()


def test_vendored_suite_matches_its_checksums():
    recorded = harness.CHECKSUMS.read_text(encoding="utf-8").splitlines()
    assert list(harness.checksum_lines()) == recorded


def test_every_test_type_and_hash_algorithm_is_vendored():
    cases = list(harness.cases())
    assert len({case.name for case in cases}) == len(cases)
    assert {case.kind for case in cases} == set(harness.CaseKind)
    assert {case.hash_algorithm for case in cases} == set(harness.HashAlgorithm)


def test_pass_counts_match_the_baseline(report, baseline):
    actual, expected = report.counts(), baseline.counts
    assert actual.total == expected.total, f"case count changed; {UPDATE}"
    assert actual.turbo >= expected.turbo, "turbo RDFC-1.0 conformance regressed"
    assert actual.turbo == expected.turbo, f"turbo improved; {UPDATE}"
    assert actual.rdflib_graded == expected.rdflib_graded, UPDATE
    assert actual.rdflib == expected.rdflib, f"RDFLib count changed; {UPDATE}"


def test_every_turbo_failure_is_ledgered_with_a_reason(report, baseline):
    ledger = {failure.case: failure.reason for failure in baseline.known_failures}
    assert set(report.failures()) == set(ledger)
    assert not [case for case, reason in ledger.items() if reason.startswith("TODO")]


def test_poison_case_fails_fast_under_the_default_budget():
    (poison,) = [case for case in harness.cases() if case.kind is harness.CaseKind.NEGATIVE]
    started = time.monotonic()
    assert harness.grade_turbo(poison).passed
    assert time.monotonic() - started < 10


def test_map_grading_rejects_wrong_identifiers(monkeypatch):
    (case,) = [case for case in harness.cases() if case.name == "test003m"]
    assert harness.grade_turbo(case).passed
    relabelled = harness.canonical_text(case).replace("_:c14n0", "_:c14n1")
    monkeypatch.setattr(harness, "canonical_text", lambda case: relabelled)
    assert not harness.grade_turbo(case).passed
