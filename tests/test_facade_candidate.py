"""Candidate facades change dependency metadata, never program behavior."""

import copy
import hashlib
import importlib.util
import io
import json
import tarfile
from pathlib import Path

import pytest


def helper():
    spec = importlib.util.spec_from_file_location(
        "facade_candidate", Path(__file__).resolve().parents[1] / "scripts/facade_candidate.py"
    )
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def manifest():
    return {
        "project": {
            "name": "pyshacl-turbo",
            "version": "1.0.0",
            "dependencies": ["rdflib>=7", "rdflib-turbo==0.2.0a9"],
            "optional-dependencies": {"test": ["pytest", "rdflib-turbo==0.2.0a9"]},
        },
        "build-system": {"build-backend": "hatchling.build"},
    }


def test_candidate_namespace_is_derived_from_both_immutable_sources():
    assert helper().candidate_version("1.0.0", "a" * 40, "b" * 40) == (
        "1.0.0+candidate." + "a" * 12 + "." + "b" * 12
    )
    with pytest.raises(ValueError, match="immutable source"):
        helper().candidate_version("1.0.0", "main", "b" * 40)


def test_only_prepared_provider_binding_and_candidate_version_can_change():
    original = manifest()
    version = "1.0.0+candidate." + "a" * 12 + "." + "b" * 12
    changed = helper().bound_manifest(original, "0.2.0a16", version, "../rdflib-turbo")
    helper().verify_manifest(original, changed, "0.2.0a16", version, "../rdflib-turbo")
    assert original == manifest()
    changed["project"]["description"] = "unrelated edit"
    with pytest.raises(ValueError, match="dependency-only"):
        helper().verify_manifest(original, changed, "0.2.0a16", version, "../rdflib-turbo")


def test_candidate_lock_rejects_unrelated_dependency_upgrades():
    original = {
        "version": 1,
        "package": [
            {"name": "pyshacl-turbo", "version": "1.0.0"},
            {"name": "rdflib-turbo", "version": "0.2.0a9"},
            {"name": "pytest", "version": "9.1.1"},
        ],
    }
    changed = copy.deepcopy(original)
    changed["package"][0]["version"] = "1.0.0+candidate.example"
    helper().verify_lock(original, changed)
    changed["package"][2]["version"] = "9.2.0"
    with pytest.raises(ValueError, match="unrelated locked dependencies"):
        helper().verify_lock(original, changed)


def source_archive(path, version, provider, *, candidate=False, code=b"original"):
    manifest = (
        f'[project]\nname="pyshacl-turbo"\nversion="{version}"\n'
        f'dependencies=["rdflib-turbo=={provider}"]\n'
        '[build-system]\nbuild-backend="hatchling.build"\n'
    )
    if candidate:
        manifest += '[tool.uv.sources]\nrdflib-turbo={path="../rdflib-turbo",editable=true}\n'
    contents = {
        "pyproject.toml": manifest.encode(),
        "uv.lock": (
            f'version=1\n[[package]]\nname="pyshacl-turbo"\nversion="{version}"\n'
            f'[[package]]\nname="rdflib-turbo"\nversion="{provider}"\n'
        ).encode(),
        "PKG-INFO": (
            f"Metadata-Version: 2.4\nName: pyshacl-turbo\nVersion: {version}\n"
            f"Requires-Dist: rdflib-turbo=={provider}\n\nSame description.\n"
        ).encode(),
        "python/pyshacl_turbo/__init__.py": code,
    }
    with tarfile.open(path, "w:gz") as output:
        for name, data in contents.items():
            member = tarfile.TarInfo("package/" + name)
            member.size = len(data)
            output.addfile(member, io.BytesIO(data))
    return hashlib.sha256(path.read_bytes()).hexdigest()


@pytest.mark.parametrize("code", [b"original", b"changed program"])
def test_candidate_original_archives_preserve_all_program_bytes(tmp_path, code):
    version = "1.0.0+candidate." + "a" * 12 + "." + "b" * 12
    base, candidate = tmp_path / "base.tar.gz", tmp_path / "candidate.tar.gz"
    base_sha = source_archive(base, "1.0.0", "0.2.0a9")
    candidate_sha = source_archive(candidate, version, "0.2.0a16", candidate=True, code=code)
    if code != b"original":
        with pytest.raises(ValueError, match="program bytes"):
            helper().verify_archives(
                base, base_sha, candidate, candidate_sha, "0.2.0a16", version, "../rdflib-turbo"
            )
    else:
        assert helper().verify_archives(
            base, base_sha, candidate, candidate_sha, "0.2.0a16", version, "../rdflib-turbo"
        )["changed"] == ["PKG-INFO", "pyproject.toml", "uv.lock"]


def test_candidate_receipt_cannot_claim_publishable_artifacts(tmp_path):
    path = tmp_path / "candidate-inputs.json"
    path.write_text(
        json.dumps(
            {
                "schema": 1,
                "kind": "test-only-facade-candidate",
                "publishable": True,
                "artifact_namespace": "candidate-facade",
            }
        )
    )
    with pytest.raises(ValueError, match="nonpublishable candidate"):
        helper().verify_artifacts(
            path, lambda wheel: pytest.fail("read payload"), "a" * 40, "b" * 40, "0.2.0a9"
        )


def test_candidate_receipt_must_match_both_requested_source_revisions(tmp_path):
    path = tmp_path / "candidate-inputs.json"
    path.write_text(
        json.dumps(
            {
                "schema": 1,
                "kind": "test-only-facade-candidate",
                "publishable": False,
                "artifact_namespace": "candidate-facade",
                "base_input": {"revision": "a" * 40},
                "provider_input": {"revision": "c" * 40},
                "provider_version": "0.2.0a9",
            }
        )
    )
    with pytest.raises(ValueError, match="immutable source inputs"):
        helper().verify_artifacts(
            path, lambda wheel: pytest.fail("read payload"), "a" * 40, "b" * 40, "0.2.0a9"
        )


def test_candidate_receipt_requires_independent_declaration_peer_authority(tmp_path):
    path = tmp_path / "candidate-inputs.json"
    path.write_text(
        json.dumps(
            {
                "schema": 1,
                "kind": "test-only-facade-candidate",
                "publishable": False,
                "artifact_namespace": "candidate-facade",
                "base_input": {"revision": "a" * 40},
                "provider_input": {"revision": "b" * 40},
                "provider_version": "0.2.0a9",
            }
        )
    )
    with pytest.raises(ValueError, match="declaration peer authority"):
        helper().verify_artifacts(
            path, lambda wheel: pytest.fail("read payload"), "a" * 40, "b" * 40, "0.2.0a9"
        )


def test_candidate_acceptance_source_uses_verified_archive_not_git(tmp_path, monkeypatch):
    module = helper()
    path = tmp_path / "candidate.tar.gz"
    version = module.candidate_version("1.0.0", "a" * 40, "b" * 40)
    sha = source_archive(path, version, "0.2.0a16", candidate=True)
    source = tmp_path / "source"
    source.mkdir()
    with tarfile.open(path) as archive:
        for member in archive:
            relative = Path(*Path(member.name).parts[1:])
            target = source / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(archive.extractfile(member).read())
    wheel = tmp_path / "candidate.whl"
    receipt = tmp_path / "candidate-inputs.json"
    record = {"candidate_archive_sha256": sha}
    receipt.write_text(json.dumps(record))
    calls = []

    def verified(*arguments):
        calls.append(arguments)
        return {
            "record": record,
            "artifacts": {"candidate_archive": path, "candidate_wheel": wheel},
            "proof": {"changed": ["PKG-INFO", "pyproject.toml", "uv.lock"]},
        }

    monkeypatch.setattr(module, "verify_artifacts", verified)
    frozen = module.freeze_source(source, wheel, receipt, None, "a" * 40, "b" * 40, "0.2.0a16")
    assert calls == [(receipt, None, "a" * 40, "b" * 40, "0.2.0a16")]
    assert frozen["kind"] == "candidate-sdist"
    assert frozen["base_revision"] == "a" * 40
    assert frozen["sha256"] == sha
    (source / "python/pyshacl_turbo/__init__.py").write_text("changed")
    with pytest.raises(ValueError, match="Source bytes differ"):
        module.freeze_source(source, wheel, receipt, None, "a" * 40, "b" * 40, "0.2.0a16")
    with pytest.raises(ValueError, match="explicit candidate wheel"):
        module.freeze_source(
            source, tmp_path / "other.whl", receipt, None, "a" * 40, "b" * 40, "0.2.0a16"
        )
