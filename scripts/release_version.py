"""Validate the Cargo release identity and expose its Python distribution spelling."""

import os
import re
import tomllib
from pathlib import Path


def python_version(version):
    match = re.fullmatch(
        r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-(alpha|beta|rc)\.(0|[1-9]\d*))?",
        version,
    )
    if not match:
        raise ValueError(f"Unsupported Cargo release version: {version!r}")
    major, minor, patch, channel, candidate = match.groups()
    base = f"{major}.{minor}.{patch}"
    return base + ({"alpha": "a", "beta": "b", "rc": "rc"}[channel] + candidate if channel else "")


def validated_versions(root, expected):
    cargo = tomllib.loads((root / "Cargo.toml").read_text())["package"]["version"]
    python = tomllib.loads((root / "pyproject.toml").read_text())["project"]["version"]
    if python_version(cargo) != python:
        raise ValueError("Rust and Python versions differ")
    if expected and cargo != expected:
        raise ValueError("Checkout does not match the prepared version")
    return cargo, python


if __name__ == "__main__":
    cargo, python = validated_versions(Path.cwd(), os.environ.get("PREPARED_VERSION", ""))
    with open(os.environ["GITHUB_OUTPUT"], "a") as output:
        output.write(f"cargo-version={cargo}\nversion={python}\n")
