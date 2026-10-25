"""Every native acceptance environment binds the retained pure YAML artifact."""

import importlib.util
import json
from pathlib import Path
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_collector_installs_yaml_in_the_single_no_build_transaction(tmp_path, monkeypatch):
    module = load("native_coverage")
    paths = [tmp_path / name for name in ("project", "native", "shacl", "owl", "yaml")]
    commands = []
    monkeypatch.setattr(module, "run", lambda args, environment: commands.append(args))
    module.install_python_cohort(*paths, {})
    assert commands == [
        ["uv", "add", "--project", paths[0], "--no-sync", *paths[1:]],
        ["uv", "sync", "--project", paths[0], "--locked", "--no-build"],
    ]


def test_collector_verifies_yaml_authority_and_loaded_payload(tmp_path, monkeypatch):
    module = load("native_coverage")
    authority, wheel, venv = [tmp_path / name for name in ("authority.json", "yaml.whl", ".venv")]
    proof = {"wheel_sha256": "a" * 64, "payload": {"yaml/__init__.py": "b" * 64}}
    calls = []
    checker = SimpleNamespace(
        verify=lambda owner, path: calls.append((owner, path)) or {"wheel": wheel, "record": {}}
    )
    monkeypatch.setattr(module, "yaml_checker", lambda: checker)
    monkeypatch.setattr(
        module,
        "run",
        lambda command, environment, capture: calls.append(command) or json.dumps(proof),
    )
    assert module.verify_installed_yaml(tmp_path, authority, ["uv", "run"], venv, {}) == proof
    assert calls[0] == calls[-1] == (tmp_path, authority)
    assert calls[1][2:5] == ["python", "-I", "-B"]
    assert str(wheel) in calls[1] and str(venv) in calls[1]


def test_changed_yaml_authority_prevents_the_isolated_probe(tmp_path, monkeypatch):
    module = load("native_coverage")

    def fail(*args):
        raise ValueError("original YAML archive changed")

    monkeypatch.setattr(module, "yaml_checker", lambda: SimpleNamespace(verify=fail))
    monkeypatch.setattr(
        module, "run", lambda *args, **kwargs: pytest.fail("ran changed dependency")
    )
    with pytest.raises(ValueError, match="original YAML"):
        module.verify_installed_yaml(tmp_path, tmp_path / "authority.json", [], tmp_path, {})


def test_workflows_forward_the_explicit_yaml_authority():
    for workflow, driver in (
        ("ci.yml", "scripts/test_wheel.py"),
        ("native-coverage.yml", "scripts/native_coverage.py"),
    ):
        lines = (ROOT / ".github/workflows" / workflow).read_text().splitlines()
        command = next(line for line in lines if driver in line)
        assert '--yaml-authority "$YAML_AUTHORITY"' in command


def test_wheel_driver_binds_same_yaml_wheel_and_checks_authority_after_suite(tmp_path, monkeypatch):
    module = load("test_wheel")
    wheel, facade, owl, yaml = [
        tmp_path / name for name in ("native.whl", "shacl.whl", "owl.whl", "yaml.whl")
    ]
    authority = tmp_path / "yaml-inputs.json"
    checks, commands = [], []
    monkeypatch.setattr(module, "require_facade_wheel", lambda path: path)
    monkeypatch.setattr(module, "require_owlrl_wheel", lambda path: path)
    monkeypatch.setattr(module, "verify_metadata", lambda path: None)
    monkeypatch.setattr(module, "verify_licenses", lambda path: None)
    checker = SimpleNamespace(
        verify=lambda owner, path: checks.append((owner, path)) or {"wheel": yaml, "record": {}}
    )
    monkeypatch.setattr(module, "yaml_checker", lambda: checker)
    monkeypatch.setattr(
        module.subprocess, "run", lambda command, **kwargs: commands.append(command)
    )
    module.test_installed_wheel(wheel, "3.15", facade, owl, authority)
    install = next(
        command for command in commands if command[:2] == ["uv", "add"] and str(wheel) in command
    )
    assert install[-4:] == list(map(str, (wheel, facade, owl, yaml)))
    verifiers = [command for command in commands if "--verify-loaded" in command]
    assert len(verifiers) == 2 and all(command[-1] == str(yaml) for command in verifiers)
    assert checks == [(module.ROOT, authority)] * 2
