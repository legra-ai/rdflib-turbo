"""Published source authority is original archive identity, never invented Git."""

import hashlib
import importlib.util
import io
import tarfile
from pathlib import Path

import pytest


def module():
    spec = importlib.util.spec_from_file_location(
        "coverage_archive", Path(__file__).resolve().parents[1] / "scripts/coverage_archive.py"
    )
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


def archive(tmp_path, names=None):
    source = tmp_path / "source"
    source.mkdir()
    path = tmp_path / "rdflib_turbo-1.0.0.tar.gz"
    names = names or {"pyproject.toml": b'[project]\nname="rdflib-turbo"\nversion="1.0.0"\n'}
    with tarfile.open(path, "w:gz") as output:
        for name, data in names.items():
            member = tarfile.TarInfo("rdflib_turbo-1.0.0/" + name)
            member.size = len(data)
            output.addfile(member, io.BytesIO(data))
            if ".." not in Path(name).parts:
                destination = source / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(data)
    return path, source, hashlib.sha256(path.read_bytes()).hexdigest()


def test_original_archive_is_authority_without_a_git_checkout(tmp_path):
    path, source, sha = archive(tmp_path)
    record = module().freeze(path, source, sha)
    assert record["kind"] == "published-sdist"
    assert record["sha256"] == sha
    assert record["version"] == "1.0.0"
    assert record["files"] == {
        "pyproject.toml": hashlib.sha256((source / "pyproject.toml").read_bytes()).hexdigest()
    }
    assert "revision" not in record


def test_candidate_facade_archive_requires_explicit_project_identity(tmp_path):
    path, source, sha = archive(
        tmp_path,
        {"pyproject.toml": b'[project]\nname="pyshacl-turbo"\nversion="1.0.0+candidate.test"\n'},
    )
    with pytest.raises(ValueError, match="native provider"):
        module().freeze(path, source, sha)
    record = module().freeze(path, source, sha, project="pyshacl-turbo")
    assert record["sha256"] == sha
    assert record["version"] == "1.0.0+candidate.test"


def test_published_owl_archive_retains_original_source_authority(tmp_path):
    path, source, sha = archive(
        tmp_path,
        {"pyproject.toml": b'[project]\nname="owlrl-turbo"\nversion="0.1.0a3"\n'},
    )
    record = module().freeze(path, source, sha, project="owlrl-turbo")
    assert record["kind"] == "published-sdist"
    assert record["version"] == "0.1.0a3"
    assert record["sha256"] == sha


def test_archive_hash_mismatch_fails_before_source_is_read(tmp_path):
    path, source, _ = archive(tmp_path)
    with pytest.raises(ValueError, match="archive SHA-256"):
        module().freeze(path, source / "missing", "0" * 64)


def test_extracted_source_tampering_is_rejected(tmp_path):
    path, source, sha = archive(tmp_path)
    (source / "pyproject.toml").write_text("changed")
    with pytest.raises(ValueError, match="Source bytes differ"):
        module().freeze(path, source, sha)


def test_archive_traversal_is_rejected_without_reading_outside_source(tmp_path):
    path, source, sha = archive(tmp_path, {"../escape": b"bad"})
    with pytest.raises(ValueError, match="archive member"):
        module().freeze(path, source, sha)


@pytest.mark.parametrize("path,sha", [("source.tar.gz", None), (None, "a" * 64)])
def test_owner_collector_rejects_partial_archive_authority_before_git(tmp_path, path, sha):
    spec = importlib.util.spec_from_file_location(
        "native_coverage", Path(__file__).resolve().parents[1] / "scripts/native_coverage.py"
    )
    collector = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(collector)
    with pytest.raises(ValueError, match="Both source archive and original SHA-256"):
        collector.source_authority(tmp_path, path, sha)
