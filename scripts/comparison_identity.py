"""Derive the owned engine identity independently from verified source bytes.

The field framing and Cargo feature spelling match the existing public paired
comparison verifier; source ownership here is explicitly local, never a Git alias.
"""

import hashlib
import importlib.util
import json
import re
import subprocess
import tomllib
from pathlib import Path, PurePosixPath

FORKS = ("oxigraph", "oxrdf", "oxsdatatypes", "spargebra", "sparopt", "spareval", "fancy-regex")


def canonical(data):
    return data.replace(b"\r\n", b"\n")


def engine_inputs(root):
    implementation = (root / "src/revision.rs").read_text()
    paths = re.findall(
        r'include_bytes!\("([^\"]+)"\)', implementation.split("fn update_source", 1)[0]
    )
    actual = {
        path.relative_to(root / "src").as_posix()
        for path in (root / "src").rglob("*.rs")
        if not path.name.endswith("_tests.rs")
    }
    if (
        not paths
        or len(paths) != len(set(paths))
        or set(paths) != actual
        or any(
            PurePosixPath(name).is_absolute() or ".." in PurePosixPath(name).parts for name in paths
        )
    ):
        raise ValueError("Fingerprint must cover the complete engine implementation")
    return paths


def fork_identity(source, package, features):
    fork = Path(package["manifest_path"]).resolve().parent
    if package["source"] is not None or not fork.is_relative_to(source / "vendor"):
        raise ValueError("Expected archive-owned patched semantic dependencies")
    result = hashlib.sha256()

    def field(data):
        result.update(canonical(data) + b"\0")

    files = [fork / "Cargo.toml", fork / "build.rs", *(fork / "src").rglob("*.rs")]
    for path in sorted(files, key=lambda path: path.relative_to(fork).parts):
        field(path.relative_to(fork).as_posix().encode())
        field(path.read_bytes())
    field(b"@shared/turbo-source-identity.rs")
    field((fork.parent / "turbo-source-identity.rs").read_bytes())
    for feature in sorted(features, key=lambda value: value.upper().replace("-", "_")):
        key = "CARGO_FEATURE_" + feature.upper().replace("-", "_")
        if package["name"] == "oxigraph" and key in {
            "CARGO_FEATURE_ROCKSDB",
            "CARGO_FEATURE_ROCKSDB_DEBUG",
            "CARGO_FEATURE_ROCKSDB_PKG_CONFIG",
        }:
            continue
        field((key + "=1").encode())
    return result.hexdigest()


def derive(source, metadata):
    source = source.resolve()
    engines = [p for p in metadata["packages"] if p["name"] == "pyshacl-turbo-engine"]
    if (
        len(engines) != 1
        or engines[0]["source"] is not None
        or Path(engines[0]["manifest_path"]).resolve() != source / "crates/engine/Cargo.toml"
    ):
        raise ValueError("Expected the archive-owned local SHACL engine")
    declared = tomllib.loads((source / "Cargo.toml").read_text())["dependencies"][
        "pyshacl-turbo-engine"
    ]
    if declared != {"path": "crates/engine"}:
        raise ValueError("Engine declaration must name the exact archive-owned engine")
    root = source / "crates/engine"
    paths = engine_inputs(root)
    digest = hashlib.sha256(canonical((root / "Cargo.toml").read_bytes()))
    for name in paths:
        digest.update(name.encode() + b"\0" + canonical((root / "src" / name).read_bytes()) + b"\0")
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    identities = {}
    for name in FORKS:
        packages = [package for package in metadata["packages"] if package["name"] == name]
        if len(packages) != 1:
            raise ValueError(f"Expected one owned semantic dependency: {name}")
        package = packages[0]
        patch = tomllib.loads((source / "Cargo.toml").read_text())["patch"]["crates-io"][name]
        if (
            set(patch) != {"path"}
            or Path(package["manifest_path"]).resolve()
            != (source / patch["path"] / "Cargo.toml").resolve()
        ):
            raise ValueError("Resolved semantic fork differs from the exact declared archive patch")
        identity = fork_identity(source, package, nodes[package["id"]]["features"])
        identities[name] = identity
        digest.update(name.encode() + b"\0" + identity.encode() + b"\0")
    return {
        "engine_source": "crates/engine",
        "engine_source_count": len(paths),
        "engine_fingerprint": digest.hexdigest(),
        "dependency_fingerprints": identities,
    }


def tool(name):
    path = Path(__file__).with_name(name + ".py")
    spec = importlib.util.spec_from_file_location("verified_" + name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def verify_receipt(receipt):
    """Reconstruct complete source/component authority before trusting compiled exports."""
    checker = tool("comparison_source")
    base = Path(receipt["wheel"]).resolve().parent.parent
    archive = Path(receipt["archive"]).resolve()
    source = Path(receipt["archive_source"]).resolve()
    metadata_path = Path(receipt["cargo_metadata"]).resolve()
    checkout = Path(receipt["source_checkout"]).resolve()
    model = receipt["model_source"]
    if (
        archive.parent != base / "sdist"
        or source.parent != base / "extracted"
        or metadata_path != base / "cargo-metadata.json"
        or checkout != base.parent / (base.name + "-source")
        or Path(model["archive"]).resolve().parent != base / "model"
    ):
        raise ValueError(
            "Comparison source roles must remain inside their exact artifact namespace"
        )
    actual = subprocess.check_output(
        ["git", "-C", checkout, "rev-parse", "HEAD"], text=True
    ).strip()
    if actual != receipt["commit"] or checker.freeze(checkout) != receipt["source_files"]:
        raise ValueError("Original public Git source authority changed")
    included = checker.audit_tree(checkout, source, receipt["source_files"])
    if not set(included) <= set(receipt["archive_files"]):
        raise ValueError("Frozen archive inventory omitted original Git source")
    if included != {name: receipt["archive_files"][name] for name in included}:
        raise ValueError("Original Git-to-archive payload authority differs")
    if checker.verify_tree(source, archive, receipt["archive_sha256"]) != receipt["archive_files"]:
        raise ValueError("Complete original source archive inventory changed")
    if checker.sha(metadata_path) != receipt["cargo_metadata_sha256"]:
        raise ValueError("Frozen compilation metadata changed")
    metadata = json.loads(metadata_path.read_text())
    roots = [
        package for package in metadata["packages"] if package["name"] == "rdflib-turbo-native"
    ]
    if (
        len(roots) != 1
        or roots[0]["source"] is not None
        or Path(roots[0]["manifest_path"]).resolve() != source / "Cargo.toml"
    ):
        raise ValueError("Metadata substituted the native source owner")
    identity = derive(source, metadata)
    if identity != receipt["expected_identity"]:
        raise ValueError("Engine source count or semantic fork fingerprints changed")
    model = tool("comparison_model").verify(source, metadata, model)
    return {"engine": identity, "model": model}
