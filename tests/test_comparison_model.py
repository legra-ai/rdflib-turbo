"""The model used to compile comparisons is an original checksum-locked registry crate."""

import copy
import importlib.util
import io
import tarfile
from pathlib import Path

import pytest


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.fixture
def owner(tmp_path):
    return load(
        "model_fixture", Path(__file__).with_name("comparison_owner_fixture.py")
    ).make_owner(tmp_path)


@pytest.fixture
def checker():
    return load(
        "comparison_model", Path(__file__).resolve().parents[1] / "scripts/comparison_model.py"
    )


def test_original_model_is_copied_unchanged_and_verified_before_and_after(owner, checker, tmp_path):
    record = checker.prepare(owner["source"], owner["metadata"], tmp_path / "retained-model")
    assert record["package"]["version"] == "6.2.0"
    assert Path(record["archive"]).read_bytes() == owner["model_archive"].read_bytes()
    assert record["archive_sha256"] == owner["model_checksum"]
    assert checker.verify(owner["source"], owner["metadata"], record) == record


@pytest.mark.parametrize(
    "tamper", ("checksum", "registry-source", "version", "metadata-version", "engine-edge")
)
def test_model_lock_and_actual_resolved_dependency_cannot_disagree(
    owner, checker, tmp_path, tamper
):
    metadata = copy.deepcopy(owner["metadata"])
    if tamper == "metadata-version":
        next(p for p in metadata["packages"] if p["id"] == "model")["version"] = "6.1.0"
    elif tamper == "engine-edge":
        metadata["resolve"]["nodes"][0]["deps"] = []
    else:
        path = owner["source"] / "Cargo.lock"
        old, new = {
            "checksum": (owner["model_checksum"], "0" * 64),
            "registry-source": (
                "registry+https://github.com/rust-lang/crates.io-index",
                "git+https://example.invalid/model",
            ),
            "version": ("6.2.0", "6.1.0"),
        }[tamper]
        path.write_text(path.read_text().replace(old, new))
    with pytest.raises(ValueError):
        checker.prepare(owner["source"], metadata, tmp_path / "retained-model")


@pytest.mark.parametrize("tamper", ("compiled-body", "original-copy", "source-extra", "lock-drift"))
def test_model_mutation_after_preparation_is_fatal(owner, checker, tmp_path, tamper):
    record = checker.prepare(owner["source"], owner["metadata"], tmp_path / "retained-model")
    path = {
        "compiled-body": owner["model"] / "src/lib.rs",
        "original-copy": Path(record["archive"]),
        "source-extra": owner["model"] / "extra.rs",
        "lock-drift": owner["source"] / "Cargo.lock",
    }[tamper]
    path.write_bytes((path.read_bytes() if path.exists() else b"") + b"changed")
    with pytest.raises(ValueError):
        checker.verify(owner["source"], owner["metadata"], record)


@pytest.mark.parametrize("kind", (tarfile.SYMTYPE, tarfile.LNKTYPE, tarfile.FIFOTYPE))
def test_archive_source_cannot_introduce_links_or_special_files(checker, tmp_path, kind):
    path = tmp_path / "unsafe.tar.gz"
    with tarfile.open(path, "w:gz") as bundle:
        info = tarfile.TarInfo("source/member")
        info.type = kind
        info.linkname = "elsewhere"
        bundle.addfile(info, io.BytesIO())
    with pytest.raises(ValueError):
        checker.archive_files(path, "source")
