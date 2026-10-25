"""Release metadata tests must use verified originals, never rebound candidate floors."""

import hashlib
import importlib.util
import io
import json
import tarfile
from pathlib import Path
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[1]


def helper():
    spec = importlib.util.spec_from_file_location(
        "facade_release_project", ROOT / "scripts/facade_release_project.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.fixture
def inputs(tmp_path, monkeypatch):
    module = helper()
    archive = tmp_path / "original.tar.gz"
    content = {
        "pyproject.toml": b'[project]\nname="pyshacl-turbo"\nversion="0.7.0"\ndependencies=["rdflib-turbo==0.2.0a17"]\n',
        "uv.lock": b"original editable lock bytes\n",
        "original.txt": b"preserved\r\n",
    }
    with tarfile.open(archive, "w:gz") as package:
        for name, data in content.items():
            info = tarfile.TarInfo("pyshacl_turbo-0.7.0/" + name)
            info.size = len(data)
            package.addfile(info, io.BytesIO(data))
    proof = {
        "record": {
            "base_input": {"revision": "a" * 40},
            "base_archive_sha256": hashlib.sha256(archive.read_bytes()).hexdigest(),
        },
        "artifacts": {"base_archive": archive},
    }
    original_load = module.load
    calls = []

    def verify(*args):
        calls.append(args)
        return proof

    monkeypatch.setattr(
        module,
        "load",
        lambda name: (
            SimpleNamespace(verify_artifacts=verify)
            if name == "facade_candidate"
            else original_load(name)
        ),
    )
    candidate = {
        "receipt": tmp_path / "candidate-inputs.json",
        "facade_revision": "a" * 40,
        "provider_revision": "b" * 40,
        "provider_version": "0.2.0a19",
    }
    output = tmp_path / "collector"
    output.mkdir()
    return module, candidate, output, content, calls


def test_only_verified_original_archive_supplies_release_metadata(inputs):
    module, candidate, output, content, calls = inputs
    authority = module.prepare(candidate, output, object())
    record = module.verify(authority)
    assert record["kind"] == "original-facade-release-metadata"
    assert record["version"] == "0.7.0"
    assert record["base_revision"] == candidate["facade_revision"]
    assert (Path(record["source"]) / "pyproject.toml").read_bytes() == content["pyproject.toml"]
    assert set(record["archive_files"]) == set(content)
    assert calls[0][2:] == ("a" * 40, "b" * 40, "0.2.0a19")
    with module.stage(authority, record) as environment:
        assert environment == {"SHACL_RELEASE_PROJECT_AUTHORITY": str(authority)}
    assert module.verify(authority, record) == record


@pytest.mark.parametrize("mutation", ["source", "extra", "archive", "record"])
def test_stage_rechecks_complete_original_authority_even_after_a_failed_consumer(inputs, mutation):
    module, candidate, output, _, _ = inputs
    authority = module.prepare(candidate, output, object())
    record = module.verify(authority)
    with pytest.raises((ValueError, RuntimeError), match=r"(?i)original|authority|inventory"):
        with module.stage(authority, record):
            if mutation == "source":
                (Path(record["source"]) / "uv.lock").write_text("changed")
            elif mutation == "extra":
                (Path(record["source"]) / "unexpected").write_text("changed")
            elif mutation == "archive":
                Path(record["archive"]).write_bytes(b"changed")
            else:
                changed = {**record, "base_revision": "c" * 40}
                authority.write_text(json.dumps(changed))
            raise LookupError("test consumer failed")


def test_noncandidate_stage_never_infers_an_original_project():
    with helper().stage(None, None) as environment:
        assert environment == {}


def test_invalid_candidate_is_rejected_before_materializing_any_source(inputs, monkeypatch):
    module, candidate, output, _, _ = inputs

    def reject(*args):
        raise ValueError("candidate failed full original artifact proof")

    monkeypatch.setattr(module, "load", lambda name: SimpleNamespace(verify_artifacts=reject))
    with pytest.raises(ValueError, match="artifact proof"):
        module.prepare(candidate, output, object())
    assert list(output.iterdir()) == []
