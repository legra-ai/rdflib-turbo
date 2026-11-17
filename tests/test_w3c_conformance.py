"""W3C RDF syntax conformance ratchet for rdflib-turbo, graded beside RDFLib.

The committed baseline holds exact per-suite pass counts for both libraries and a
ledger of every turbo failure. Counts may neither drop nor rise silently: rewrite
the baseline with ``python scripts/w3c_conformance.py --write`` and review it.
"""

import importlib.util
import sys
from importlib.metadata import version
from pathlib import Path
from types import ModuleType

import pytest

ROOT = Path(__file__).resolve().parents[1]


def load_harness() -> ModuleType:
    spec = importlib.util.spec_from_file_location(
        "w3c_conformance", ROOT / "scripts" / "w3c_conformance.py"
    )
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    # Dataclass annotations resolve through sys.modules.
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


harness = load_harness()
UPDATE = "rewrite tests/conformance-baseline.json with `python scripts/w3c_conformance.py --write`"


@pytest.fixture(scope="module")
def report():
    return harness.Report.run()


@pytest.fixture(scope="module")
def baseline():
    return harness.Baseline.load()


def test_vendored_suites_match_their_checksums():
    recorded = harness.CHECKSUMS.read_text(encoding="utf-8").splitlines()
    assert list(harness.checksum_lines()) == recorded


def test_case_names_are_unique_within_each_suite():
    for suite in harness.Suite:
        names = [case.id.name for case in suite.cases()]
        assert len(names) == len(set(names)), suite


def test_rdflib_column_is_graded_against_the_pinned_rdflib(baseline):
    assert version("rdflib") == baseline.rdflib_version


def test_pass_counts_match_the_baseline(report, baseline):
    actual = report.counts()
    assert actual.keys() == baseline.suites.keys()
    for suite, expected in baseline.suites.items():
        counts = actual[suite]
        assert counts.total == expected.total, f"{suite.name} case count changed; {UPDATE}"
        assert counts.turbo >= expected.turbo, f"{suite.name}: turbo conformance regressed"
        assert counts.turbo == expected.turbo, f"{suite.name}: turbo improved; {UPDATE}"
        assert counts.rdflib == expected.rdflib, f"{suite.name}: RDFLib count changed; {UPDATE}"


def test_every_turbo_failure_is_ledgered(report, baseline):
    ledger = baseline.ledger()
    failures = report.failures(harness.Engine.TURBO)
    unledgered = {str(case): outcome.detail for case, outcome in failures.items()}
    for case in ledger:
        unledgered.pop(str(case), None)
    assert not unledgered, f"Unledgered turbo failures: {unledgered}"


def test_no_ledger_entry_is_stale(report, baseline):
    failures = report.failures(harness.Engine.TURBO)
    stale = [str(case) for case in baseline.ledger() if case not in failures]
    assert not stale, f"Ledgered cases now pass; remove them: {stale}"


def test_ledger_states_where_turbo_is_worse_than_rdflib(report, baseline):
    wrong = [
        str(failure.id)
        for failure in baseline.known_failures
        if failure.id in report.outcomes
        and failure.rdflib_passes != report.rdflib_passes(failure.id)
    ]
    assert not wrong, f"rdflib_passes disagrees with RDFLib's actual outcome: {wrong}"


def test_ledger_reasons_are_written(baseline):
    vague = [
        str(failure.id) for failure in baseline.known_failures if failure.reason.startswith("TODO")
    ]
    assert not vague, f"Ledgered failures need a reason: {vague}"


def test_isomorphism_relabels_blank_nodes_but_keeps_terms_and_graphs():
    one = harness.Engine.RDFLIB.parse_text('_:a <urn:p> "x" <urn:g> .\n')
    relabelled = harness.Engine.TURBO.parse_text('_:b <urn:p> "x" <urn:g> .\n')
    other_literal = harness.Engine.TURBO.parse_text('_:b <urn:p> "y" <urn:g> .\n')
    other_graph = harness.Engine.TURBO.parse_text('_:b <urn:p> "x" .\n')
    assert harness.QuadSet.of(one).isomorphic(harness.QuadSet.of(relabelled))
    assert not harness.QuadSet.of(one).isomorphic(harness.QuadSet.of(other_literal))
    assert not harness.QuadSet.of(one).isomorphic(harness.QuadSet.of(other_graph))


def test_every_eval_case_is_compared_against_its_expected_result(report):
    evaluated = [
        row[harness.Engine.TURBO].detail
        for case, row in report.outcomes.items()
        if case.suite is harness.Suite.RDF11_TURTLE and row[harness.Engine.TURBO].passed
    ]
    assert evaluated.count("isomorphic") > 100
