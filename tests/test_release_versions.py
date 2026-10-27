"""Cargo preparation must preserve Python candidate spelling and synchronize locks."""

import importlib.util
import re
import subprocess
import tomllib
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, ROOT / "scripts" / f"{name}.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.mark.parametrize(
    "cargo,python",
    [
        ("0.2.0-alpha.12", "0.2.0a12"),
        ("0.2.0-beta.2", "0.2.0b2"),
        ("0.2.0-rc.3", "0.2.0rc3"),
        ("0.2.0", "0.2.0"),
    ],
)
def test_python_version(cargo, python):
    assert load("release_version").python_version(cargo) == python


@pytest.mark.parametrize(
    "version", ["0.2.0a1", "01.2.0", "0.2.0-preview.1", "0.2.0-alpha.01", "0.2.0+build"]
)
def test_unsupported_versions_fail(version):
    with pytest.raises(ValueError):
        load("release_version").python_version(version)


def test_synchronization_declares_and_updates_both_companion_files(tmp_path, monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / "scripts"))
    (tmp_path / "Cargo.toml").write_text(
        '[package.metadata.version-info]\nadditional_files = ["pyproject.toml", "uv.lock"]\n'
    )
    (tmp_path / "pyproject.toml").write_text('[project]\nversion = "0.2.0a1"\n')
    (tmp_path / "uv.lock").write_text("fixture")
    commands = []
    monkeypatch.setattr(
        subprocess, "run", lambda command, **kwargs: commands.append((command, kwargs))
    )
    load("sync_version").synchronize(tmp_path, "0.2.0-alpha.2")
    assert (
        tomllib.loads((tmp_path / "pyproject.toml").read_text())["project"]["version"] == "0.2.0a2"
    )
    assert commands == [(["uv", "lock", "--project", str(tmp_path)], {"check": True})]


def test_synchronization_fails_without_declared_lockfile(tmp_path, monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / "scripts"))
    (tmp_path / "Cargo.toml").write_text(
        '[package.metadata.version-info]\nadditional_files = ["pyproject.toml"]\n'
    )
    with pytest.raises(ValueError, match=r"uv\.lock"):
        load("sync_version").synchronize(tmp_path, "0.2.0-alpha.2")


def test_version_outputs_refuse_disagreement(tmp_path):
    (tmp_path / "Cargo.toml").write_text('[package]\nversion = "0.2.0-alpha.2"\n')
    (tmp_path / "pyproject.toml").write_text('[project]\nversion = "0.2.0a1"\n')
    with pytest.raises(ValueError, match="versions differ"):
        load("release_version").validated_versions(tmp_path, "0.2.0-alpha.2")


def test_version_outputs_refuse_a_different_prepared_version(tmp_path):
    (tmp_path / "Cargo.toml").write_text('[package]\nversion = "0.2.0-alpha.2"\n')
    (tmp_path / "pyproject.toml").write_text('[project]\nversion = "0.2.0a2"\n')
    with pytest.raises(ValueError, match="prepared version"):
        load("release_version").validated_versions(tmp_path, "0.2.0-alpha.3")


def test_prepared_version_uses_root_package_not_independent_workspace(tmp_path):
    (tmp_path / "Cargo.toml").write_text(
        '[package]\nversion = "0.2.0-alpha.19"\n[workspace.package]\nversion = "0.7.0"\n'
    )
    (tmp_path / "pyproject.toml").write_text('[project]\nversion = "0.2.0a19"\n')
    release = load("release_version")
    assert release.validated_versions(tmp_path, "0.2.0-alpha.19") == (
        "0.2.0-alpha.19",
        "0.2.0a19",
    )
    with pytest.raises(ValueError, match="prepared version"):
        release.validated_versions(tmp_path, "0.7.0")


def _assert_shared_release_action_pins(workflow):
    for action in ["prepare-cargo-release", "tag-tested-release"]:
        revisions = re.findall(
            rf"(?m)^\s*(?:-\s+)?uses:\s+legra-ai/github-actions/\.github/actions/{action}@([^\s#]+)",
            workflow,
        )
        assert len(revisions) == 1, f"Expected exactly one shared {action} step"
        assert re.fullmatch(r"[0-9a-f]{40}", revisions[0]), f"Unpinned shared {action} step"


def test_shared_release_actions_accept_independent_immutable_updates():
    workflow = (
        "    uses: legra-ai/github-actions/.github/actions/prepare-cargo-release@"
        + "a" * 40
        + "\n  - uses: legra-ai/github-actions/.github/actions/tag-tested-release@"
        + "b" * 40
        + "\n"
    )
    _assert_shared_release_action_pins(workflow)


@pytest.mark.parametrize("action", ["prepare-cargo-release", "tag-tested-release"])
@pytest.mark.parametrize("revision", ["main", "v1", "a" * 39, "a" * 41, "G" * 40, ""])
def test_shared_release_actions_reject_floating_or_malformed_revisions(action, revision):
    workflow = "\n".join(
        f"uses: legra-ai/github-actions/.github/actions/{name}@"
        + (revision if name == action else "a" * 40)
        for name in ["prepare-cargo-release", "tag-tested-release"]
    )
    with pytest.raises(AssertionError):
        _assert_shared_release_action_pins(workflow)


def test_shared_release_actions_reject_missing_or_duplicate_steps():
    step = "uses: legra-ai/github-actions/.github/actions/prepare-cargo-release@" + "a" * 40
    tag = "uses: legra-ai/github-actions/.github/actions/tag-tested-release@" + "b" * 40
    for workflow in ["", step, step + "\n" + tag + "\n" + step]:
        with pytest.raises(AssertionError):
            _assert_shared_release_action_pins(workflow)


def test_main_release_uses_shared_planner_and_successful_test_gate():
    workflow = (ROOT / ".github/workflows/ci.yml").read_text()
    assert "inputs.publish" not in workflow
    assert "steps.release.outputs.release" in workflow
    assert "needs.prepare.outputs.release == 'true'" in workflow
    _assert_shared_release_action_pins(workflow)
    assert "needs: [prepare, test]" in workflow
    assert "cancel-in-progress: false" in workflow
    assert workflow.count("tag-format: pep440") == 2
    assert not (ROOT / "scripts/prepare_release.py").exists()
    assert not (ROOT / "scripts/tag_release.py").exists()
