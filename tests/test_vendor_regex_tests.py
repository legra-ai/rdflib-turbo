"""The sole native owner preserves every upstream and Python matcher gate."""

import importlib
import json
import subprocess
from pathlib import Path
from types import SimpleNamespace

import pytest


def runner(monkeypatch):
    monkeypatch.syspath_prepend(str(Path(__file__).resolve().parents[1]))
    return importlib.import_module("scripts.vendor_regex_tests")


def test_standalone_matcher_manifest_is_explicitly_outside_owner_workspace():
    root = Path(__file__).resolve().parents[1]
    matcher = root / "vendor/fancy-regex-0.19.2"
    result = subprocess.run(
        [
            "cargo",
            "metadata",
            "--no-deps",
            "--locked",
            "--offline",
            "--format-version",
            "1",
            "--manifest-path",
            str(matcher / "Cargo.toml"),
        ],
        cwd=root,
        check=True,
        capture_output=True,
        text=True,
    )
    metadata = json.loads(result.stdout)
    assert Path(metadata["workspace_root"]) == matcher
    assert len(metadata["workspace_members"]) == 1


def test_regex_owner_runs_all_three_profiles_and_pinned_formatter(tmp_path, monkeypatch):
    module = runner(monkeypatch)
    (tmp_path / "Cargo.toml").write_text('[patch.crates-io]\noxrdf={path="vendor/oxrdf"}\n')
    (tmp_path / "rustfmt-toolchain").write_text("nightly-2026-10-16\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        return SimpleNamespace(stdout="\n".join(f"{name}: test" for name in module.ALL_GUARDS))

    monkeypatch.setattr(module.subprocess, "run", cargo)
    module.run(tmp_path)
    assert len(calls) == 7
    tests = calls[1:6:2]
    assert all("--locked" in call for call in tests)
    assert not any("patch.crates-io" in argument for call in tests for argument in call)
    assert "--no-default-features" not in tests[0]
    assert "--lib" not in tests[0]
    assert tests[1][-1] == "std,perf,unicode"
    assert tests[2][-2:] == ["std,perf,unicode,python-re", "python_re_tests"]
    assert all("--no-default-features" in call and "--lib" in call for call in tests[1:])
    assert calls[-1] == [
        "cargo",
        "+nightly-2026-10-16",
        "fmt",
        "--manifest-path",
        str(tmp_path / "vendor/fancy-regex-0.19.2/Cargo.toml"),
        "--",
        "--check",
    ]
    workflow = (Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml").read_text()
    assert "python -m scripts.vendor_regex_tests" in workflow


def test_missing_python_vm_guard_is_fatal_before_filtered_tests(tmp_path, monkeypatch):
    module = runner(monkeypatch)
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        return SimpleNamespace(stdout=module.UNICODE_GUARD + ": test")

    monkeypatch.setattr(module.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing matcher regressions"):
        module.run(tmp_path)
    assert len(calls) == 5
