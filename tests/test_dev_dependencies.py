"""Development and CI must exercise the declared pytest version range."""

import shlex
import tomllib
from pathlib import Path


def test_make_dev_uses_the_declared_pytest_requirement():
    root = Path(__file__).resolve().parents[1]
    metadata = tomllib.loads((root / "pyproject.toml").read_text())
    expected = next(
        requirement
        for requirement in metadata["project"]["optional-dependencies"]["test"]
        if requirement.startswith("pytest>")
    )
    commands = next(
        line
        for line in (root / "Makefile").read_text().splitlines()
        if line.strip().startswith("uv pip install ")
    )
    assert expected in shlex.split(commands)
