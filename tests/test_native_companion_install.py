"""The collector installs reasoning before collecting any provider Python tests."""

import importlib.util
import json
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


def helper():
    spec = importlib.util.spec_from_file_location(
        "native_companion_install", ROOT / "scripts/native_coverage.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_all_three_original_wheels_are_bound_before_the_one_sync(tmp_path, monkeypatch):
    module = helper()
    paths = [
        tmp_path / name for name in ("project", "native.whl", "facade.whl", "owl.whl", "yaml.whl")
    ]
    commands = []
    monkeypatch.setattr(module, "run", lambda args, environment: commands.append(list(args)))
    module.install_python_cohort(*paths, {})
    assert len(commands) == 2
    assert commands[0] == ["uv", "add", "--project", paths[0], "--no-sync", *paths[1:]]
    assert commands[1] == ["uv", "sync", "--project", paths[0], "--locked", "--no-build"]


def test_no_reasoning_source_can_substitute_for_a_missing_original_wheel(tmp_path):
    with pytest.raises(ValueError, match="original OWL"):
        helper().freeze_owl_inputs(tmp_path / "source", None)


def test_loaded_reasoning_is_verified_in_the_isolated_environment(tmp_path, monkeypatch):
    module = helper()
    wheel, venv = tmp_path / "owl.whl", tmp_path / "venv"
    expected = {"wheel_sha256": "a" * 64, "package_payload": {"owlrl_turbo/__init__.py": "b" * 64}}
    monkeypatch.setattr(module, "freeze_owl_inputs", lambda *args: expected)
    commands = []
    proof = {
        "package": str(venv / "site-packages/owlrl_turbo"),
        "payload": expected["package_payload"],
        "wheel_sha256": expected["wheel_sha256"],
    }
    monkeypatch.setattr(
        module,
        "run",
        lambda command, environment, capture: commands.append(command) or json.dumps(proof),
    )
    actual = module.verify_installed_companion(
        tmp_path / "source", wheel, expected, ["uv", "run", "--locked", "--no-sync"], venv, {}
    )
    assert actual == proof
    assert commands[0][4:7] == ["python", "-I", "-B"]
    assert str(wheel) in commands[0] and str(venv) in commands[0]


def test_wheel_or_source_drift_stops_before_running_any_python_probe(tmp_path, monkeypatch):
    module = helper()
    monkeypatch.setattr(module, "freeze_owl_inputs", lambda *args: {"changed": True})
    monkeypatch.setattr(
        module, "run", lambda *args, **kwargs: pytest.fail("ran a changed original")
    )
    with pytest.raises(RuntimeError, match="OWL companion inputs changed"):
        module.verify_installed_companion(
            tmp_path, tmp_path / "wheel.whl", {"original": True}, [], tmp_path / "venv", {}
        )
