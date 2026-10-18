"""CI must execute every allocation budget and reject an empty Cargo selection."""

import importlib
from pathlib import Path

import pytest


@pytest.fixture
def budgets(monkeypatch):
    monkeypatch.syspath_prepend(str(Path(__file__).resolve().parents[1]))
    return importlib.import_module("scripts.allocation_budgets")


def test_missing_budget_fails_before_running_tests(budgets, monkeypatch):
    calls = []

    def run(arguments, environment, *, capture=False):
        calls.append(arguments)
        return "another_test: test\n"

    monkeypatch.setattr(budgets, "run", run)
    with pytest.raises(RuntimeError, match="Missing allocation budget"):
        budgets.run_budgets({})
    assert len(calls) == 1


def test_budgets_run_in_separate_processes_with_exact_selection(budgets, monkeypatch):
    calls = []
    environment = {"PYO3_PYTHON": "/selected/python"}

    def run(arguments, received_environment, *, capture=False):
        assert received_environment is environment
        calls.append((arguments, capture))
        return "\n".join(f"{test}: test" for test in budgets.TESTS) if capture else None

    monkeypatch.setattr(budgets, "run", run)
    budgets.run_budgets(environment)
    assert len(calls) == 1 + len(budgets.TESTS)
    for (arguments, capture), test in zip(calls[1:], budgets.TESTS, strict=True):
        assert not capture
        assert arguments[-6:] == [
            test,
            "--",
            "--ignored",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ]


def test_failed_budget_stops_remaining_processes(budgets, monkeypatch):
    calls = []

    def run(arguments, environment, *, capture=False):
        if capture:
            return "\n".join(f"{test}: test" for test in budgets.TESTS)
        calls.append(arguments)
        raise RuntimeError("budget exceeded")

    monkeypatch.setattr(budgets, "run", run)
    with pytest.raises(RuntimeError, match="budget exceeded"):
        budgets.run_budgets({})
    assert len(calls) == 1
