"""Standalone Cargo tests must retain their declared native dependency graphs."""

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


@pytest.mark.parametrize("relative", ("tests/store-contract", "tests/build-metadata"))
def test_nested_contract_locks_include_every_required_local_crate_edge(relative):
    directory = ROOT / relative
    manifest = tomllib.loads((directory / "Cargo.toml").read_text())
    lock = tomllib.loads((directory / "Cargo.lock").read_text())
    local = {directory / "Cargo.toml"}
    for declarations in (
        manifest.get("dependencies", {}),
        manifest.get("patch", {}).get("crates-io", {}),
    ):
        for definition in declarations.values():
            if isinstance(definition, dict) and "path" in definition:
                local.add((directory / definition["path"] / "Cargo.toml").resolve())
    for path in sorted(local):
        crate = tomllib.loads(path.read_text())
        name = crate["package"]["name"]
        entries = [
            entry for entry in lock["package"] if entry["name"] == name and "source" not in entry
        ]
        # Unused patches are not part of this contract's resolved dependency graph.
        if not entries and path != directory / "Cargo.toml":
            continue
        assert len(entries) == 1, f"{relative}: expected one local package {name}"
        locked = {dependency.split(" ", 1)[0] for dependency in entries[0].get("dependencies", [])}
        for section in ("dependencies", "build-dependencies"):
            for alias, definition in crate.get(section, {}).items():
                if isinstance(definition, dict) and definition.get("optional", False):
                    continue
                required = (
                    definition.get("package", alias) if isinstance(definition, dict) else alias
                )
                assert required in locked, f"{relative}: {name} is missing required {required}"
