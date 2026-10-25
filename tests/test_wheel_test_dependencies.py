"""Offline identity tests require an explicit locked host dependency preflight."""

import importlib.util
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


def helper():
    spec = importlib.util.spec_from_file_location(
        "wheel_test_dependencies", ROOT / "scripts/wheel_test_dependencies.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_locked_fetch_preserves_source_and_caller_cargo_policy(tmp_path, monkeypatch):
    module = helper()
    frozen = {"revision": "a" * 40, "files": {"Cargo.lock": "original"}}
    checks, calls = [], []
    monkeypatch.setattr(module, "freeze_source", lambda root: checks.append(root) or frozen)
    monkeypatch.setenv("CARGO_NET_OFFLINE", "true")
    monkeypatch.setattr(
        module.subprocess, "run", lambda command, **kwargs: calls.append((command, kwargs))
    )
    assert module.prepare(tmp_path) == frozen
    assert checks == [tmp_path, tmp_path]
    assert calls == [
        (
            ["cargo", "fetch", "--locked", "--manifest-path", str(tmp_path / "Cargo.toml")],
            {"cwd": tmp_path, "check": True},
        )
    ]


def test_fetch_failure_is_not_retried_or_replaced_with_online_resolution(tmp_path, monkeypatch):
    module = helper()
    monkeypatch.setattr(module, "freeze_source", lambda root: {"frozen": True})
    calls = []

    def fail(command, **kwargs):
        calls.append(command)
        raise subprocess.CalledProcessError(101, command)

    monkeypatch.setattr(module.subprocess, "run", fail)
    with pytest.raises(subprocess.CalledProcessError):
        module.prepare(tmp_path)
    assert len(calls) == 1
    assert "--locked" in calls[0]
    assert "--offline" not in calls[0]


def test_source_or_lock_drift_is_rejected_after_prefetch(tmp_path, monkeypatch):
    module = helper()
    frozen = iter([{"Cargo.lock": "original"}, {"Cargo.lock": "changed"}])
    monkeypatch.setattr(module, "freeze_source", lambda root: next(frozen))
    monkeypatch.setattr(module.subprocess, "run", lambda *args, **kwargs: None)
    with pytest.raises(RuntimeError, match=r"source.*changed"):
        module.prepare(tmp_path)
