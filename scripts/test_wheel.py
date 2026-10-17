"""Install and test built wheels in isolated Python 3.11 and 3.14 environments."""

import subprocess
import tempfile
from pathlib import Path
from zipfile import ZipFile

root = Path(__file__).resolve().parents[1]
wheels = list((root / "dist").glob("*.whl"))
if len(wheels) != 1:
    raise ValueError("Expected exactly one built wheel")
# Native persistence bundles RocksDB, LevelDB-derived code and the LZ4 library.
# Check the actual artifact rather than assuming license-files globs were applied.
with ZipFile(wheels[0]) as archive:
    members = archive.namelist()
    for name in ("LICENSE-APACHE", "LICENSE-LEVELDB", "LICENSE-LZ4"):
        relative = f"vendor/oxrocksdb-sys-0.5.11-notices/{name}"
        matches = [member for member in members if member.endswith(f"/licenses/{relative}")]
        if len(matches) != 1:
            raise RuntimeError(f"Wheel is missing bundled native license: {relative}")
        if archive.read(matches[0]) != (root / relative).read_bytes():
            raise RuntimeError(f"Bundled native license changed: {relative}")
for version in ("3.11", "3.14"):
    with tempfile.TemporaryDirectory() as directory:
        project = Path(directory) / "project"
        subprocess.run(
            [
                "uv",
                "init",
                "--bare",
                "--no-workspace",
                "--vcs",
                "none",
                "--python",
                version,
                str(project),
            ],
            check=True,
        )
        subprocess.run(["uv", "python", "pin", "--project", str(project), version], check=True)
        subprocess.run(
            [
                "uv",
                "add",
                "--project",
                str(project),
                "--extra",
                "test",
                "--no-sync",
                str(wheels[0]),
            ],
            check=True,
        )
        subprocess.run(["uv", "sync", "--project", str(project), "--locked"], check=True)
        subprocess.run(
            [
                "uv",
                "run",
                "--project",
                str(project),
                "--locked",
                "pytest",
                str(root / "tests"),
                "-q",
                "--import-mode=importlib",
                "--cov=rdflib_turbo",
                "--cov-branch",
                "--cov-fail-under=100",
            ],
            cwd=directory,
            check=True,
        )
