"""Source archives must not publish private plans or local experiment bundles."""

import importlib
import io
import tarfile
from pathlib import Path

import pytest


@pytest.fixture
def audit(monkeypatch):
    monkeypatch.syspath_prepend(str(Path(__file__).resolve().parents[1]))
    return importlib.import_module("scripts.audit_sdist").audit


def archive(tmp_path, relative):
    path = tmp_path / "source.tar.gz"
    content = b"archive content"
    with tarfile.open(path, "w:gz") as output:
        entry = tarfile.TarInfo(f"rdflib_turbo-0.2.0a1/{relative}")
        entry.size = len(content)
        output.addfile(entry, io.BytesIO(content))
    return path


@pytest.mark.parametrize(
    "relative",
    ["docs/plans/drop-in-parity.md", "docs/parity-evidence/native-graph/manifest.json"],
)
def test_private_documents_are_rejected(audit, tmp_path, relative):
    with pytest.raises(ValueError, match="Private document"):
        audit(archive(tmp_path, relative))


def test_public_architecture_documents_are_allowed(audit, tmp_path):
    audit(archive(tmp_path, "docs/adr/0001-shared-native-runtime.md"))


def test_archive_traversal_is_rejected(audit, tmp_path):
    with pytest.raises(ValueError, match="Unsafe archive path"):
        audit(archive(tmp_path, "../private.md"))
