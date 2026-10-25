"""Standalone Cargo tests must retain their declared native dependency graphs."""

import subprocess
import tomllib
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
LOCKFILES = (
    "vendor/oxigraph-0.5.13/Cargo.lock",
    "vendor/oxrdf-0.3.5/Cargo.lock",
    "vendor/spareval-0.2.9/Cargo.lock",
    "vendor/fancy-regex-0.20.0/Cargo.lock",
)


def test_every_standalone_vendor_test_lock_is_owned_by_git(tmp_path):
    # Exercise the shipped ignore policy in a synthetic Git checkout, so this
    # guard also runs from an original source archive without repository metadata.
    repository = tmp_path / "repository"
    repository.mkdir()
    subprocess.run(["git", "init", "--quiet", str(repository)], check=True)
    for relative in (".gitignore", "vendor/.gitignore"):
        source = ROOT / relative
        if source.exists():
            target = repository / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(source.read_bytes())
    patches = tomllib.loads((ROOT / "Cargo.toml").read_text())["patch"]["crates-io"]
    standalone = set()
    for patch in patches.values():
        directory = ROOT / patch["path"]
        manifest = tomllib.loads((directory / "Cargo.toml").read_text())
        if "workspace" in manifest:
            relative = (directory / "Cargo.lock").relative_to(ROOT).as_posix()
            standalone.add(relative)
            target = repository / relative
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes((ROOT / relative).read_bytes())
            ignore = directory / ".gitignore"
            if ignore.exists():
                (target.parent / ".gitignore").write_bytes(ignore.read_bytes())
    subprocess.run(["git", "-c", "core.autocrlf=false", "add", "--all"], cwd=repository, check=True)
    for relative in sorted(standalone):
        tracked = subprocess.run(
            ["git", "ls-files", "--error-unmatch", "--", relative],
            cwd=repository,
            capture_output=True,
            text=True,
        )
        assert tracked.returncode == 0, f"Unattributed standalone test input: {relative}"
        staged = subprocess.run(
            ["git", "show", f":{relative}"], cwd=repository, capture_output=True, check=True
        ).stdout
        assert staged == (ROOT / relative).read_bytes(), f"Unstaged test lock: {relative}"
    assert standalone == set(LOCKFILES), "Keep the required standalone test inventory complete"


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
