"""Published license bytes must survive a Windows-configured Git checkout."""

import subprocess
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
    # bounded: the fixed set of notices shipped for bundled native components.
    paths = [
        "LICENSE-APACHE",
        "vendor/oxigraph-0.5.11/LICENSE-MIT",
        "vendor/oxrocksdb-sys-0.5.11-notices/LICENSE-APACHE",
        "vendor/oxrocksdb-sys-0.5.11-notices/LICENSE-LEVELDB",
        "vendor/oxrocksdb-sys-0.5.11-notices/LICENSE-LZ4",
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
