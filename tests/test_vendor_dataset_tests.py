"""Dataset regressions must execute the patched vendor with both RDF modes."""

import importlib
import subprocess
from pathlib import Path
from types import SimpleNamespace

import pytest


@pytest.fixture
def runner(monkeypatch):
    monkeypatch.syspath_prepend(str(Path(__file__).resolve().parents[1]))
    return importlib.import_module("scripts.vendor_dataset_tests")


def test_runs_real_patched_dataset_units_and_doctests_in_both_modes(runner, monkeypatch, tmp_path):
    (tmp_path / "Cargo.toml").write_text('[patch.crates-io]\noxrdf={path="vendor/oxrdf"}\n')
    calls = []

    def cargo(arguments, **kwargs):
        calls.append((arguments, kwargs))
        return SimpleNamespace(stdout="\n".join(f"{name}: test" for name in runner.REQUIRED_TESTS))

    monkeypatch.setattr(runner.subprocess, "run", cargo)
    runner.run(tmp_path)
    assert len(calls) == 4
    for listing, execution in (calls[:2], calls[2:]):
        assert listing[0][-2:] == ["--", "--list"]
        arguments, options = execution
        assert options["cwd"] == tmp_path and options["check"] is True
        assert "--locked" in arguments and "--lib" not in arguments
        assert arguments[arguments.index("--manifest-path") + 1] == str(
            tmp_path / "vendor/oxrdf-0.3.5/Cargo.toml"
        )
        assert any(str(tmp_path / "vendor/oxrdf") in value for value in arguments)
    assert calls[1][0][-1] == "rdfc-10,rdf-10-string-identity"
    assert calls[3][0][-1] == "rdfc-10,rdf-10-string-identity,rdf-12,serde"


def test_missing_dataset_regression_stops_before_execution(runner, monkeypatch, tmp_path):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        return SimpleNamespace(stdout="unrelated_test: test\n")

    monkeypatch.setattr(runner.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native dataset regressions"):
        runner.run(tmp_path)
    assert len(calls) == 1


def test_dataset_failure_propagates_without_running_next_mode(runner, monkeypatch, tmp_path):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []
    failure = subprocess.CalledProcessError(8, ["cargo"])

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        if len(calls) == 2:
            raise failure
        return SimpleNamespace(stdout="\n".join(f"{name}: test" for name in runner.REQUIRED_TESTS))

    monkeypatch.setattr(runner.subprocess, "run", cargo)
    with pytest.raises(subprocess.CalledProcessError) as result:
        runner.run(tmp_path)
    assert result.value is failure
    assert len(calls) == 2


def test_generalized_equality_is_a_mandatory_dataset_regression(runner):
    assert (
        "dataset::generalized_tests::generalized_equality_compares_full_roles_and_graph_kinds_independent_of_insert_order"
        in runner.REQUIRED_TESTS
    )


@pytest.mark.parametrize(
    "name",
    (
        "generalized_streaming_removal_preserves_first_arcs_and_current_insertions_on_rollback",
        "generalized_streaming_removal_keeps_exact_graph_kinds_and_all_pattern_masks",
        "generalized_streaming_removal_absence_keeps_additive_fast_path_and_unwind_restores_words",
    ),
)
def test_streaming_removal_is_a_mandatory_dataset_regression(runner, name):
    assert f"dataset::generalized_remove_tests::{name}" in runner.REQUIRED_TESTS
