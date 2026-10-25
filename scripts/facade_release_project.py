"""Preserve original release metadata separately from test-only runtime rebinding."""

import hashlib
import importlib.util
import json
import re
import tomllib
from contextlib import contextmanager
from pathlib import Path


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def inventory(source):
    if source.is_symlink() or not source.is_dir():
        raise ValueError("Original release metadata source must be a regular directory")
    files = {}  # bounded: the fixed original facade source archive, never RDF inputs.
    for path in source.rglob("*"):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError("Original release metadata inventory contains a non-regular path")
        if path.is_file():
            with path.open("rb") as stream:
                files[path.relative_to(source).as_posix()] = hashlib.file_digest(
                    stream, "sha256"
                ).hexdigest()
    return files


def prepare(candidate, output, payload_reader):
    output = output.resolve(strict=True)
    verified = load("facade_candidate").verify_artifacts(
        candidate["receipt"],
        payload_reader,
        candidate["facade_revision"],
        candidate["provider_revision"],
        candidate["provider_version"],
    )
    archive = verified["artifacts"]["base_archive"].resolve(strict=True)
    source = output.resolve(strict=True) / "release-metadata-source"
    load("extract_source").extract(archive, source)
    original = verified["record"]
    proof = load("coverage_archive").freeze(
        archive, source, original["base_archive_sha256"], project="pyshacl-turbo"
    )
    record = {
        "schema": 1,
        "kind": "original-facade-release-metadata",
        "project": "pyshacl-turbo",
        "source": str(source),
        "base_revision": original["base_input"]["revision"],
        "archive": str(archive),
        "archive_sha256": proof["sha256"],
        "archive_files": proof["files"],
        "version": proof["version"],
    }
    destination = output / "release-metadata-authority.json"
    with destination.open("x") as stream:
        stream.write(json.dumps(record, indent=2) + "\n")
    verify(destination, record)
    return destination


def verify(authority, expected=None):
    record = json.loads(Path(authority).read_text())
    if expected is not None and record != expected:
        raise ValueError("Original release metadata authority record changed")
    if (
        set(record)
        != {
            "schema",
            "kind",
            "project",
            "source",
            "base_revision",
            "archive",
            "archive_sha256",
            "archive_files",
            "version",
        }
        or record["schema"] != 1
        or record["kind"] != "original-facade-release-metadata"
        or record["project"] != "pyshacl-turbo"
        or not re.fullmatch(r"[0-9a-f]{40}", record["base_revision"])
    ):
        raise ValueError("Expected explicit original release metadata authority")
    source, archive = Path(record["source"]), Path(record["archive"])
    if archive.is_symlink() or not archive.is_file():
        raise ValueError("Original release metadata archive must remain a regular file")
    proof = load("coverage_archive").freeze(
        archive, source, record["archive_sha256"], project="pyshacl-turbo"
    )
    if proof["files"] != record["archive_files"] or inventory(source) != record["archive_files"]:
        raise ValueError("Original release metadata source inventory changed")
    if not {"pyproject.toml", "uv.lock"} <= record["archive_files"].keys():
        raise ValueError("Original release metadata requires its complete manifest and lock")
    manifest = tomllib.loads((source / "pyproject.toml").read_text())
    if manifest["project"]["version"] != record["version"]:
        raise ValueError("Original release metadata version changed")
    return record


@contextmanager
def stage(authority, expected):
    if authority is None:
        if expected is not None:
            raise ValueError("Original release metadata authority is incomplete")
        yield {}
        return
    verify(authority, expected)
    try:
        yield {"SHACL_RELEASE_PROJECT_AUTHORITY": str(authority)}
    finally:
        verify(authority, expected)
