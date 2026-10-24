"""Provider CI consumes one explicitly unpublishable, immutable facade cohort."""

import importlib.util
import json
import subprocess
from pathlib import Path
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[1]
REVISION = "a" * 40


def helper():
    spec = importlib.util.spec_from_file_location("ci_facade", ROOT / "scripts/ci_facade.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_policy_requires_the_public_repository_and_an_immutable_revision(tmp_path):
    policy = tmp_path / "policy.json"
    document = {
        "facade": {"repository": "https://github.com/legra-ai/pyshacl-turbo", "revision": REVISION},
        "owlrl": {"repository": "https://github.com/legra-ai/owlrl-turbo", "revision": REVISION},
    }
    policy.write_text(json.dumps(document))
    assert helper().policy(policy) == document
    for defect in ("main", "file:///private/source", "/Users/private/checkout"):
        broken = json.loads(json.dumps(document))
        field = "revision" if defect == "main" else "repository"
        broken["facade"][field] = defect
        policy.write_text(json.dumps(broken))
        with pytest.raises(ValueError):
            helper().policy(policy)


def test_evidence_bundle_retains_archives_and_receipts_without_clones(tmp_path):
    module = helper()
    source, output = tmp_path / "candidate-facade-build", tmp_path / "candidate-facade-evidence"
    source.mkdir()
    names = {
        "base_archive": "originals/base.tar.gz",
        "base_wheel": "originals/base.whl",
        "candidate_archive": "candidate-artifacts/candidate.tar.gz",
        "candidate_wheel": "candidate-artifacts/candidate.whl",
    }
    record = {**names, "declaration": {"archive": names["base_archive"]}}
    retained = [
        *names.values(),
        "candidate-inputs.json",
        "dependency-binding.patch",
        "declaration-requirements.lock",
    ]
    for name in [
        *retained,
        "cohort/candidate-worktree/.git/config",
        "cohort/candidate-source/package.py",
    ]:
        path = source / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(name)
    module.retain_evidence(source, output, record)
    assert {
        path.relative_to(output).as_posix() for path in output.rglob("*") if path.is_file()
    } == set(retained)
    assert all((output / name).read_bytes() == (source / name).read_bytes() for name in retained)


def test_evidence_bundle_rejects_a_path_outside_its_namespace(tmp_path):
    source = tmp_path / "candidate-facade-build"
    source.mkdir()
    for name in (
        "candidate-inputs.json",
        "dependency-binding.patch",
        "declaration-requirements.lock",
    ):
        (source / name).write_text("fixture")
    record = dict.fromkeys(
        ("base_archive", "base_wheel", "candidate_archive", "candidate_wheel"), "../private"
    )
    record["declaration"] = {"archive": "../private"}
    with pytest.raises(ValueError):
        helper().retain_evidence(source, tmp_path / "candidate-facade-evidence", record)


def test_consumer_checks_real_provider_inventory_before_extracting_or_exporting(
    tmp_path, monkeypatch
):
    module = helper()
    monkeypatch.setattr(module, "policy", lambda: {"facade": {"revision": REVISION}})
    owner = {"revision": "b" * 40, "files": {"src/lib.rs": "actual"}}
    monkeypatch.setattr(module, "owner_input", lambda: owner)
    manifest = {"project": {"version": "0.2.0a1"}}
    monkeypatch.setattr(module, "owner_manifest", lambda: manifest)
    verified = {
        "record": {"provider_input": {**owner, "files": {"src/lib.rs": "tampered"}}},
        "artifacts": {},
    }
    calls = []

    def verify(*args):
        calls.append(args)
        return verified

    dependencies = {
        "facade_candidate": SimpleNamespace(verify_artifacts=verify),
        "native_coverage": SimpleNamespace(facade_payload=object()),
    }
    monkeypatch.setattr(module, "load", lambda name: dependencies[name])
    bundle = tmp_path / "candidate-facade-evidence"
    bundle.mkdir()
    with pytest.raises(ValueError, match="provider source"):
        module.consume(bundle)
    assert calls[0][2:] == (REVISION, owner["revision"], "0.2.0a1")


def test_environment_lines_cannot_inject_other_github_commands(tmp_path):
    destination = tmp_path / "environment"
    module = helper()
    with pytest.raises(ValueError):
        module.write_environment(destination, {"SHACL_WHEEL": "good\nOTHER=bad"})
    assert not destination.exists()
    module.write_environment(destination, {"SHACL_WHEEL": "/tmp/original.whl"})
    assert destination.read_text() == "SHACL_WHEEL=/tmp/original.whl\n"


def test_consumer_git_launch_does_not_use_the_posix_only_coverage_launcher(monkeypatch):
    module = helper()
    calls = []

    def launch(command, **kwargs):
        calls.append((command, kwargs))
        return subprocess.CompletedProcess(command, 0, stdout="immutable-revision\n")

    monkeypatch.setattr(module.subprocess, "run", launch)
    assert module.run(["git", "rev-parse", "HEAD"], capture=True) == "immutable-revision\n"
    assert calls[0][1] == {"check": True, "text": True, "stdout": subprocess.PIPE}
