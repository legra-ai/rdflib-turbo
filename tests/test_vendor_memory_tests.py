"""The native memory unit runner uses the actual vendor library and project patches."""

import importlib
import json
import subprocess
from pathlib import Path
from types import SimpleNamespace

import pytest


@pytest.fixture
def vendor_memory_tests(monkeypatch):
    monkeypatch.syspath_prepend(str(Path(__file__).resolve().parents[1]))
    return importlib.import_module("scripts.vendor_memory_tests")


def test_vendor_memory_runner_applies_current_root_patches_and_locked_native_tests(
    tmp_path, monkeypatch, vendor_memory_tests
):
    root = tmp_path / "project with spaces"
    root.mkdir()
    (root / "Cargo.toml").write_text(
        '[patch.crates-io]\noxrdf = { path = "vendor/custom-rdf" }\n'
        'fancy-regex = { git = "https://example.org/engine", rev = "abc123" }\n'
    )
    calls = []

    def cargo(arguments, *, cwd, check, **kwargs):
        calls.append((arguments, cwd, check))
        return SimpleNamespace(
            stdout="\n".join(
                f"{name}: test"
                for name in (
                    *vendor_memory_tests.REQUIRED_TESTS,
                    "storage::numeric_encoder::tests::str_hash_stability",
                    "storage::rocksdb::upstream_hash_tests::exact_hash_declarations_are_rejected_before_upstream_migration",
                )
            )
        )

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    vendor_memory_tests.run(root)
    assert len(calls) == 4
    assert calls[0][0][-2:] == ["--", "--list"]
    arguments, directory, checked = calls[1]
    assert directory == root and checked is True
    assert arguments[0] == "cargo"
    assert "--locked" in arguments and "--lib" in arguments
    assert "storage::" in arguments
    assert arguments[arguments.index("--features") + 1] == "exact-term-storage,rocksdb"
    assert calls[3][0][calls[3][0].index("--features") + 1] == "rocksdb"
    assert arguments[arguments.index("--manifest-path") + 1] == str(
        root / "vendor/oxigraph-0.5.11/Cargo.toml"
    )
    config = [arguments[i + 1] for i, argument in enumerate(arguments) if argument == "--config"]
    assert f"patch.crates-io.oxrdf.path={json.dumps(str(root / 'vendor/custom-rdf'))}" in config
    assert 'patch.crates-io.fancy-regex.git="https://example.org/engine"' in config
    assert 'patch.crates-io.fancy-regex.rev="abc123"' in config


def test_vendor_memory_runner_preserves_native_failure(tmp_path, monkeypatch, vendor_memory_tests):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    failure = subprocess.CalledProcessError(7, ["cargo"])

    def cargo(*args, **kwargs):
        raise failure

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(subprocess.CalledProcessError) as captured:
        vendor_memory_tests.run(tmp_path)
    assert captured.value is failure


def test_missing_instrumented_regressions_fail_before_execution(
    tmp_path, monkeypatch, vendor_memory_tests
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        return SimpleNamespace(stdout="storage::memory::tests::test_range: test\n")

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native storage regressions"):
        vendor_memory_tests.run(tmp_path)
    assert len(calls) == 1


def test_missing_upstream_hash_regression_fails_inventory(
    tmp_path, monkeypatch, vendor_memory_tests
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        return SimpleNamespace(
            stdout="\n".join(f"{name}: test" for name in vendor_memory_tests.REQUIRED_TESTS)
        )

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native storage regressions"):
        vendor_memory_tests.run(tmp_path)


def test_missing_lexical_hash_regression_fails_inventory(
    tmp_path, monkeypatch, vendor_memory_tests
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        return SimpleNamespace(
            stdout="\n".join(
                f"{name}: test"
                for name in vendor_memory_tests.REQUIRED_TESTS
                if "lexical_hash" not in name
            )
        )

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native storage regressions"):
        vendor_memory_tests.run(tmp_path)
    assert len(calls) == 1


def test_missing_upstream_store_guard_fails_inventory(tmp_path, monkeypatch, vendor_memory_tests):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        return SimpleNamespace(
            stdout="\n".join(
                f"{name}: test"
                for name in (
                    *vendor_memory_tests.REQUIRED_TESTS,
                    "storage::numeric_encoder::tests::str_hash_stability",
                )
            )
        )

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native storage regressions"):
        vendor_memory_tests.run(tmp_path)
    assert len(calls) == 3
