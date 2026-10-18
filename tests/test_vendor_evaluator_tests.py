"""Evaluator CI must exercise isolated feature modes and actual patched code."""

import importlib
from pathlib import Path
from types import SimpleNamespace

import pytest


@pytest.fixture
def runner(monkeypatch):
    monkeypatch.syspath_prepend(str(Path(__file__).resolve().parents[1]))
    return importlib.import_module("scripts.vendor_evaluator_tests")


def test_actual_vendor_feature_modes_require_their_semantic_guards(runner, monkeypatch, tmp_path):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        return SimpleNamespace(stdout="\n".join(f"{name}: test" for name in runner.ALL_GUARDS))

    monkeypatch.setattr(runner.subprocess, "run", cargo)
    runner.run(tmp_path)
    assert len(calls) == 6
    for arguments in calls[1::2]:
        assert "--locked" in arguments and "--lib" in arguments
        assert "--no-default-features" in arguments and "--manifest-path" in arguments
        assert "-p" not in arguments
        assert str(tmp_path / "vendor/spareval-0.2.7/Cargo.toml") in arguments
    assert "--features" not in calls[1]
    assert calls[3][-1] == "exact-term-builtins,rdf-set-union"
    assert calls[5][-1] == "exact-term-builtins,rdf-set-union,sparql-12"


def test_missing_mode_specific_guard_fails_before_tests(runner, monkeypatch, tmp_path):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        names = [name for name in runner.ALL_GUARDS if "upstream_terms_default" not in name]
        return SimpleNamespace(stdout="\n".join(f"{name}: test" for name in names))

    monkeypatch.setattr(runner.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing evaluator regressions"):
        runner.run(tmp_path)
    assert len(calls) == 1
