"""Independently derive the source identity compiled into comparison wheels."""

import hashlib
import re
import subprocess
import tomllib
from pathlib import Path


def canonical(data):
    return data.replace(b"\r\n", b"\n")


def derive(source, metadata):
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    (engine,) = [p for p in metadata["packages"] if p["name"] == "pyshacl-turbo-engine"]
    root = Path(engine["manifest_path"]).parent
    revision = engine["source"].split("#")[-1]
    declared = tomllib.loads((source / "Cargo.toml").read_text())["dependencies"][
        "pyshacl-turbo-engine"
    ]
    if declared["rev"] != revision:
        raise ValueError("Resolved engine source differs from immutable manifest revision")
    actual = subprocess.check_output(
        ["git", "-C", str(root), "rev-parse", "HEAD"], text=True
    ).strip()
    dirty = subprocess.check_output(
        ["git", "-C", str(root), "status", "--porcelain"], text=True
    ).strip()
    if actual != revision or dirty not in ("", "?? .cargo-ok"):
        raise ValueError("Engine checkout is changed or does not match its resolved commit")
    implementation = (root / "src/revision.rs").read_text()
    paths = re.findall(
        r'include_bytes!\("([^\"]+)"\)', implementation.split("fn update_source", 1)[0]
    )
    if not paths or len(paths) != len(set(paths)):
        raise ValueError("Expected unique engine fingerprint source inputs")
    digest = hashlib.sha256(canonical((root / "Cargo.toml").read_bytes()))
    for name in paths:
        digest.update(name.encode() + b"\0" + canonical((root / "src" / name).read_bytes()) + b"\0")
    forks = {}
    for name in ("oxigraph", "oxrdf", "oxsdatatypes", "spargebra", "sparopt", "spareval"):
        (package,) = [p for p in metadata["packages"] if p["name"] == name]
        fork = Path(package["manifest_path"]).parent
        if package["source"] is not None or not fork.is_relative_to(source):
            raise ValueError("Expected archive-owned patched dependency")
        field_hash = hashlib.sha256()

        def field(data):
            field_hash.update(canonical(data) + b"\0")

        files = [fork / "Cargo.toml", fork / "build.rs", *(fork / "src").rglob("*.rs")]
        for path in sorted(files, key=lambda path: path.relative_to(fork).parts):
            field(path.relative_to(fork).as_posix().encode())
            field(path.read_bytes())
        field(b"@shared/turbo-source-identity.rs")
        field((fork.parent / "turbo-source-identity.rs").read_bytes())
        for feature in sorted(
            nodes[package["id"]]["features"], key=lambda value: value.upper().replace("-", "_")
        ):
            key = "CARGO_FEATURE_" + feature.upper().replace("-", "_")
            if name == "oxigraph" and key in {
                "CARGO_FEATURE_ROCKSDB",
                "CARGO_FEATURE_ROCKSDB_DEBUG",
                "CARGO_FEATURE_ROCKSDB_PKG_CONFIG",
            }:
                continue
            field((key + "=1").encode())
        forks[name] = field_hash.hexdigest()
        digest.update(name.encode() + b"\0" + forks[name].encode() + b"\0")
    return {
        "engine_revision": revision,
        "engine_fingerprint": digest.hexdigest(),
        "engine_source_count": len(paths),
        "dependency_fingerprints": forks,
    }
