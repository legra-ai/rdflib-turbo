"""Declaration reconstruction uses explicit policy, raw Git and original uv archives."""

import hashlib
import importlib.util
import json
import shutil
import subprocess
import tarfile
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


def helper():
    spec = importlib.util.spec_from_file_location(
        "declaration_peer", ROOT / "scripts/declaration_peer.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def git(source, *args):
    return subprocess.check_output(["git", "-C", str(source), *args]).decode().strip()


@pytest.fixture
def original(tmp_path):
    source = tmp_path / "public-fixture"
    source.mkdir()
    git(source, "init")
    git(source, "config", "core.autocrlf", "false")
    git(source, "config", "user.email", "fixture@example.invalid")
    git(source, "config", "user.name", "Fixture")
    (source / "pyproject.toml").write_text(
        '[project]\nname="pyshacl-turbo"\nversion="0.7.0"\n[build-system]\nbuild-backend="hatchling.build"\nrequires=["hatchling"]\n'
    )
    (source / "raw.txt").write_bytes(b"raw\r\noriginal\r\n")
    (source / "tool.sh").write_text("#!/bin/sh\nexit 0\n")
    (source / "tool.sh").chmod(0o755)
    git(source, "add", ".")
    git(source, "commit", "-m", "fixture")
    revision = git(source, "rev-parse", "HEAD")
    owner = tmp_path / "owner"
    (owner / "scripts").mkdir(parents=True)
    (owner / "uv.lock").write_text('[[package]]\nname="pyshacl-turbo"\nversion="0.7.0"\n')
    (owner / "scripts/ci-facade.json").write_text(
        json.dumps({"declaration": {"repository": str(source), "revision": revision}})
    )
    return owner, source, revision


def arrange(monkeypatch, original):
    module = helper()
    monkeypatch.setattr(
        module,
        "selected",
        lambda owner: json.loads((owner / "scripts/ci-facade.json").read_text())["declaration"],
    )
    calls = []

    def build(source, output, python):
        calls.append((source, python))
        output.mkdir()
        archive = output / "pyshacl_turbo-0.7.0.tar.gz"
        with tarfile.open(archive, "w:gz") as package:
            package.add(source, arcname="pyshacl_turbo-0.7.0")
        return archive

    monkeypatch.setattr(module, "build", build)
    return module, calls


def test_raw_git_bytes_modes_original_archive_and_relocated_source(original, tmp_path, monkeypatch):
    module, calls = arrange(monkeypatch, original)
    owner, _source, revision = original
    record = module.prepare(owner, tmp_path / "reconstructed", "3.13")
    assert record["role"] == "uv-export-declaration"
    assert record["authority"]["revision"] == revision
    assert (
        record["authority"]["files"]["raw.txt"]
        == hashlib.sha256(b"raw\r\noriginal\r\n").hexdigest()
    )
    assert record["authority"]["modes"]["tool.sh"] == "100755"
    assert (Path(record["source"]) / "raw.txt").read_bytes() == b"raw\r\noriginal\r\n"
    relocated = tmp_path / "renamed-source"
    shutil.move(record["source"], relocated)
    record["source"] = str(relocated)
    assert module.verify(owner, record) == record
    assert calls[0][1] == "3.13"
    (relocated / "extra.txt").write_text("unrecorded")
    with pytest.raises(ValueError, match="inventory"):
        module.verify(owner, record, initial=False)


def test_locked_version_drift_fails_before_any_build(original, tmp_path, monkeypatch):
    module, calls = arrange(monkeypatch, original)
    owner, _, _ = original
    (owner / "uv.lock").write_text('[[package]]\nname="pyshacl-turbo"\nversion="0.8.0"\n')
    with pytest.raises(ValueError, match="locked"):
        module.prepare(owner, tmp_path / "reconstructed", "3.13")
    assert calls == []


def test_reconstruction_rejects_archive_or_policy_drift(original, tmp_path, monkeypatch):
    module, _ = arrange(monkeypatch, original)
    owner, _, _ = original
    record = module.prepare(owner, tmp_path / "reconstructed", "3.13")
    policy = json.loads((owner / "scripts/ci-facade.json").read_text())
    policy["declaration"]["revision"] = "a" * 40
    (owner / "scripts/ci-facade.json").write_text(json.dumps(policy))
    with pytest.raises(ValueError, match="authority"):
        module.verify(owner, record)


def test_raw_materialization_rejects_symlink_blob(original, tmp_path, monkeypatch):
    module, calls = arrange(monkeypatch, original)
    owner, source, _ = original
    (source / "unsafe").symlink_to("raw.txt")
    git(source, "add", "unsafe")
    git(source, "commit", "-m", "link")
    policy = json.loads((owner / "scripts/ci-facade.json").read_text())
    policy["declaration"]["revision"] = git(source, "rev-parse", "HEAD")
    (owner / "scripts/ci-facade.json").write_text(json.dumps(policy))
    with pytest.raises(ValueError, match="regular Git"):
        module.prepare(owner, tmp_path / "reconstructed", "3.13")
    assert calls == []


def test_mandatory_policy_does_not_infer_declaration_from_facade(tmp_path):
    owner = tmp_path / "owner"
    (owner / "scripts").mkdir(parents=True)
    (owner / "scripts/ci-facade.json").write_text(
        json.dumps(
            {
                "facade": {
                    "repository": "https://github.com/legra-ai/pyshacl-turbo",
                    "revision": "a" * 40,
                },
                "owlrl": {
                    "repository": "https://github.com/legra-ai/owlrl-turbo",
                    "revision": "b" * 40,
                },
            }
        )
    )
    with pytest.raises(ValueError, match="declaration"):
        helper().selected(owner)


def test_uv_sdist_command_disables_source_overrides(tmp_path, monkeypatch):
    module = helper()
    source, output = tmp_path / "raw", tmp_path / "artifacts"
    source.mkdir()
    calls = []

    def launch(command, **kwargs):
        calls.append(command)
        (output / "original.tar.gz").write_bytes(b"archive")

    monkeypatch.setattr(module.subprocess, "run", launch)
    module.build(source, output, "3.13")
    assert calls == [
        [
            "uv",
            "build",
            "--sdist",
            "--no-sources",
            "--python",
            "3.13",
            "--out-dir",
            str(output),
            str(source),
        ]
    ]


@pytest.mark.parametrize("mutation", ["bytes", "mode", "extra"])
def test_retained_raw_build_source_remains_frozen(original, tmp_path, monkeypatch, mutation):
    module, _ = arrange(monkeypatch, original)
    owner, _, _ = original
    record = module.prepare(owner, tmp_path / "reconstructed", "3.13")
    raw = Path(record["build"]["source"])
    if mutation == "bytes":
        (raw / "raw.txt").write_bytes(b"changed")
    elif mutation == "mode":
        (raw / "tool.sh").chmod(0o644)
    else:
        (raw / "unrecorded").write_text("changed")
    with pytest.raises(ValueError, match="raw source"):
        module.verify(owner, record, initial=False)
