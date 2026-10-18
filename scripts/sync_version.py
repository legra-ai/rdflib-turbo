"""Declared cargo-version-info hook: synchronize Python metadata and its lock."""

import re
import subprocess
import sys
import tomllib
from pathlib import Path

from release_version import python_version


def synchronize(root, version):
    version = python_version(version)
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    declared = manifest["package"]["metadata"]["version-info"]["additional_files"]
    for name in ("pyproject.toml", "uv.lock"):
        if name not in declared:
            raise ValueError(f"{name} must be declared to cargo-version-info")
    for name in ("pyproject.toml", "uv.lock"):
        if not (root / name).is_file():
            raise FileNotFoundError(root / name)
    path = root / "pyproject.toml"
    content, count = re.subn(
        r'^version = "[^"]+"$', f'version = "{version}"', path.read_text(), flags=re.MULTILINE
    )
    if count != 1:
        raise ValueError("Expected exactly one Python project version")
    path.write_text(content)
    subprocess.run(["uv", "lock", "--project", str(root)], check=True)


if __name__ == "__main__":
    synchronize(Path(__file__).resolve().parents[1], sys.argv[1])
