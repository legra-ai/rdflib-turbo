"""Bind comparison compilation to the actual lock-pinned original registry model."""

import importlib.util
import re
import shutil
import tomllib
from pathlib import Path


def source_checker():
    path = Path(__file__).with_name("comparison_source.py")
    spec = importlib.util.spec_from_file_location("comparison_model_source", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def inputs(source, metadata):
    source = source.resolve()
    manifest = tomllib.loads((source / "crates/engine/Cargo.toml").read_text())
    declared = manifest["dependencies"]["shacl-structural-model"]
    if not isinstance(declared, str) or re.fullmatch(r"=[0-9]+\.[0-9]+\.[0-9]+", declared) is None:
        raise ValueError("Model must have an exact engine dependency declaration")
    packages = [p for p in metadata["packages"] if p["name"] == "shacl-structural-model"]
    entries = [
        p
        for p in tomllib.loads((source / "Cargo.lock").read_text())["package"]
        if p["name"] == "shacl-structural-model"
    ]
    engines = [p for p in metadata["packages"] if p["name"] == "pyshacl-turbo-engine"]
    if len(packages) != 1 or len(entries) != 1 or len(engines) != 1:
        raise ValueError("Expected exactly one locked and resolved model owner")
    package, entry = packages[0], entries[0]
    nodes = [node for node in metadata["resolve"]["nodes"] if node["id"] == engines[0]["id"]]
    if (
        package["version"] != entry["version"]
        or package["version"] != declared[1:]
        or not isinstance(package["source"], str)
        or not package["source"].startswith("registry+")
        or package["source"] != entry.get("source")
        or re.fullmatch(r"[0-9a-f]{64}", entry.get("checksum", "")) is None
        or len(nodes) != 1
        or not any(edge["pkg"] == package["id"] for edge in nodes[0]["deps"])
    ):
        raise ValueError("Resolved model does not match its exact engine declaration and lock")
    root = Path(package["manifest_path"]).resolve().parent
    if (
        root.name != "shacl-structural-model-" + package["version"]
        or root.parent.parent.name != "src"
        or root.parents[2].name != "registry"
    ):
        raise ValueError("Model metadata must identify the actual registry source tree")
    archive = root.parents[2] / "cache" / root.parent.name / (root.name + ".crate")
    return root, archive, entry


def prepare(source, metadata, output):
    checker = source_checker()
    root, original, entry = inputs(source, metadata)
    files = checker.verify_tree(root, original, entry["checksum"], cargo_marker=True)
    output.mkdir()
    retained = output / original.name
    shutil.copyfile(original, retained)
    record = {
        "kind": "original-locked-registry-model",
        "package": entry,
        "owner_lock_sha256": checker.sha(source / "Cargo.lock"),
        "source": str(root),
        "archive": str(retained),
        "archive_sha256": entry["checksum"],
        "archive_files": files,
    }
    return verify(source, metadata, record)


def verify(source, metadata, record):
    checker = source_checker()
    root, _, entry = inputs(source, metadata)
    archive = Path(record["archive"])
    if (
        record["kind"] != "original-locked-registry-model"
        or record["package"] != entry
        or record["source"] != str(root)
        or archive.name != root.name + ".crate"
        or record["owner_lock_sha256"] != checker.sha(source / "Cargo.lock")
        or record["archive_sha256"] != entry["checksum"]
    ):
        raise ValueError("Frozen model authority differs from the actual compilation inputs")
    if (
        checker.verify_tree(root, archive, entry["checksum"], cargo_marker=True)
        != record["archive_files"]
    ):
        raise ValueError("Model source differs from the frozen original payload")
    return record


def archive_files(archive, prefix):
    return source_checker().archive_files(archive, prefix)
