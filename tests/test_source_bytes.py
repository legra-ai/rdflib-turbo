"""Published license bytes must survive a Windows-configured Git checkout."""

import hashlib
import json
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_native_licenses_survive_crlf_checkouts(tmp_path):
    repository = tmp_path / "repository"
    repository.mkdir()

    def git(*arguments, data=None):
        return subprocess.run(
            ["git", "-C", str(repository), *arguments],
            input=data,
            capture_output=True,
            check=True,
        ).stdout

    git("init", "--quiet")
    git("config", "core.autocrlf", "true")
    git("config", "core.eol", "crlf")
    # bounded: fixed native notices plus representative C++ and CMake source bytes.
    paths = [
        "LICENSE-APACHE",
        "vendor/oxigraph-0.5.11/LICENSE-MIT",
        "vendor/oxrocksdb-sys-0.5.11-notices/LICENSE-APACHE",
        "vendor/oxrocksdb-sys-0.5.11-notices/LICENSE-LEVELDB",
        "vendor/oxrocksdb-sys-0.5.11-notices/LICENSE-LZ4",
        "vendor/oxrocksdb-sys-0.5.11/rocksdb/LICENSE.Apache",
        "vendor/oxrocksdb-sys-0.5.11/rocksdb/COPYING",
        "vendor/oxrocksdb-sys-0.5.11/rocksdb/LICENSE.leveldb",
        "vendor/oxrocksdb-sys-0.5.11/lz4/LICENSE",
        "vendor/oxrocksdb-sys-0.5.11/rocksdb/util/thread_operation.h",
        "vendor/oxrocksdb-sys-0.5.11/rocksdb/cmake/modules/CxxFlags.cmake",
    ]
    attributes = ROOT / ".gitattributes"
    if attributes.exists():
        (repository / ".gitattributes").write_bytes(attributes.read_bytes())
        git("add", ".gitattributes")
    for relative in paths:
        original = (ROOT / relative).read_bytes()
        object_id = git("hash-object", "-w", "--stdin", data=original).decode().strip()
        git("update-index", "--add", "--cacheinfo", f"100644,{object_id},{relative}")
    checkout = tmp_path / "checkout"
    checkout.mkdir()
    git("checkout-index", "--all", f"--prefix={checkout.as_posix()}/")
    for relative in paths:
        assert (checkout / relative).read_bytes() == (ROOT / relative).read_bytes(), relative


def test_vendored_native_source_matches_published_archive_manifest():
    vendor = ROOT / "vendor/oxrocksdb-sys-0.5.11"
    manifest = json.loads((vendor / ".cargo-checksum.json").read_text())
    assert manifest["package"] == (
        "01fcf5ae1e1295a612bef72fc64cd81e1ac3d1bd3243ef84d330f2962a8335e1"
    )
    # bounded: the fixed files of the published 0.5.11 crate plus its local patch.
    for relative, expected in manifest["files"].items():
        with (vendor / relative).open("rb") as source:
            actual = hashlib.file_digest(source, "sha256").hexdigest()
        assert actual == expected, relative


def test_standalone_persistence_contract_uses_vendored_backend():
    contract = ROOT / "tests/store-contract"
    configuration = tomllib.loads((contract / "Cargo.toml").read_text())
    patches = configuration["patch"]["crates-io"]
    assert "oxrocksdb-sys" in patches, "Persistence contract must exercise the patched backend"
    backend = (contract / patches["oxrocksdb-sys"]["path"]).resolve()
    assert backend == (ROOT / "vendor/oxrocksdb-sys-0.5.11").resolve()
    packages = tomllib.loads((contract / "Cargo.lock").read_text())["package"]
    package = next(package for package in packages if package["name"] == "oxrocksdb-sys")
    assert package["version"] == "0.5.11"
    assert "source" not in package, "Contract lockfile still selects the registry backend"
