"""Standalone Cargo tests must use the same immutable Git patches as the runtime."""

import tomllib
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
LOCKFILES = (
    "vendor/oxigraph-0.5.11/Cargo.lock",
    "vendor/oxrdf-0.3.4/Cargo.lock",
    "vendor/spareval-0.2.7/Cargo.lock",
)


@pytest.mark.parametrize("relative", LOCKFILES)
def test_standalone_lockfile_git_patches_match_the_runtime(relative):
    patches = tomllib.loads((ROOT / "Cargo.toml").read_text())["patch"]["crates-io"]
    lock = tomllib.loads((ROOT / relative).read_text())
    entries = [*lock.get("package", []), *lock.get("patch", {}).get("unused", [])]
    for package, patch in patches.items():
        if "git" not in patch:
            continue
        expected = f"git+{patch['git']}?rev={patch['rev']}#{patch['rev']}"
        sources = [entry["source"] for entry in entries if entry["name"] == package]
        assert sources == [expected], f"{relative}: stale Git source for {package}"
