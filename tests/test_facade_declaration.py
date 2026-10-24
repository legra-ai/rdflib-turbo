"""Candidate acceptance and original lock peers have disjoint source authorities."""

import hashlib
import importlib.util
import io
import tarfile
from pathlib import Path

import pytest


def helper():
    path = Path(__file__).resolve().parents[1] / "scripts/facade_declaration.py"
    spec = importlib.util.spec_from_file_location("facade_declaration", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sources(tmp_path):
    owner, facade = tmp_path / "owner", tmp_path / "current"
    owner.mkdir()
    facade.mkdir()
    (owner / "uv.lock").write_text('[[package]]\nname="pyshacl-turbo"\nversion="0.6.1"\n')
    (facade / "pyproject.toml").write_text('[project]\nname="pyshacl-turbo"\nversion="0.6.2"\n')
    return owner, facade


def test_different_prepared_version_requires_independent_declaration_authority(tmp_path):
    owner, facade = sources(tmp_path)
    with pytest.raises(ValueError, match="declaration peer"):
        helper().select(owner, facade, {"revision": "a" * 40, "files": {}}, None)


def test_same_source_role_requires_matching_original_locked_version(tmp_path):
    owner, facade = sources(tmp_path)
    (facade / "pyproject.toml").write_text('[project]\nname="pyshacl-turbo"\nversion="0.6.1"\n')
    frozen = {"revision": "a" * 40, "files": {}}
    selected = helper().select(owner, facade, frozen, None)
    assert selected == {"source": facade, "authority": {"kind": "git", **frozen}}


def test_original_archive_declaration_never_requires_or_fabricates_git(tmp_path):
    owner, facade = sources(tmp_path)
    declared = tmp_path / "declared"
    declared.mkdir()
    data = b'[project]\nname="pyshacl-turbo"\nversion="0.6.1"\n'
    (declared / "pyproject.toml").write_bytes(data)
    archive = tmp_path / "pyshacl_turbo-0.6.1.tar.gz"
    with tarfile.open(archive, "w:gz") as output:
        member = tarfile.TarInfo("package/pyproject.toml")
        member.size = len(data)
        output.addfile(member, io.BytesIO(data))
    sha = hashlib.sha256(archive.read_bytes()).hexdigest()
    selected = helper().select(
        owner,
        facade,
        {"revision": "a" * 40, "files": {}},
        lambda *args: pytest.fail("Git declaration authority was requested"),
        source=declared,
        archive=archive,
        sha256=sha,
    )
    assert selected["source"] == declared
    assert selected["authority"]["kind"] == "original-sdist"
    assert selected["authority"]["sha256"] == sha
    assert "revision" not in selected["authority"]


@pytest.mark.parametrize(
    "revision,archive,sha",
    [("a" * 40, "source.tar.gz", "b" * 64), (None, "source.tar.gz", None), (None, None, None)],
)
def test_explicit_declaration_authority_is_complete_and_disjoint(tmp_path, revision, archive, sha):
    owner, facade = sources(tmp_path)
    with pytest.raises(ValueError, match="one complete declaration authority"):
        helper().select(
            owner,
            facade,
            {"revision": "a" * 40, "files": {}},
            None,
            source=tmp_path / "missing",
            revision=revision,
            archive=archive,
            sha256=sha,
        )
