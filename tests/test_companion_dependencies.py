"""The provider declares every Python package imported during test collection."""

import tomllib
from pathlib import Path

from packaging.requirements import Requirement

ROOT = Path(__file__).resolve().parents[1]


def test_published_owl_test_dependency_is_explicit_without_a_third_source_override():
    manifest = tomllib.loads((ROOT / "pyproject.toml").read_text())
    assert "owlrl-turbo>=0.2.0a1,<0.3" in manifest["project"]["optional-dependencies"]["test"]
    requirements = [
        Requirement(value) for value in manifest["project"]["optional-dependencies"]["test"]
    ]
    (requirement,) = [value for value in requirements if value.name == "owlrl-turbo"]
    assert requirement.url is None
    assert "owlrl-turbo==0.2.0a1" in manifest["dependency-groups"]["coverage"]
    assert "owlrl-turbo" not in manifest["tool"]["uv"]["sources"]
    lock = tomllib.loads((ROOT / "uv.lock").read_text())
    (owl,) = [row for row in lock["package"] if row["name"] == "owlrl-turbo"]
    assert owl["version"] == "0.2.0a1"
    assert requirement.specifier.contains(owl["version"], prereleases=True)
    assert owl["source"] == {"registry": "https://pypi.org/simple"}
