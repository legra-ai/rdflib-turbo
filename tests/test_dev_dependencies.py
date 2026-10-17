"""Installed test tooling must respect the project's published pytest contract."""

import tomllib
from importlib.metadata import version
from pathlib import Path

from packaging.requirements import Requirement
from packaging.version import Version


def test_installed_pytest_satisfies_the_declared_test_extra():
    root = Path(__file__).resolve().parents[1]
    metadata = tomllib.loads((root / "pyproject.toml").read_text())
    requirements = [
        Requirement(value) for value in metadata["project"]["optional-dependencies"]["test"]
    ]
    expected = next(requirement for requirement in requirements if requirement.name == "pytest")
    assert Version(version("pytest")) in expected.specifier
