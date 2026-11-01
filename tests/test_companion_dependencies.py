"""The provider declares every Python package imported during test collection."""

import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_published_owl_test_dependency_is_explicit_without_a_third_source_override():
    manifest = tomllib.loads((ROOT / "pyproject.toml").read_text())
    assert "owlrl-turbo==0.2.0a1" in manifest["project"]["optional-dependencies"]["test"]
    assert "owlrl-turbo==0.2.0a1" in manifest["dependency-groups"]["coverage"]
    assert "owlrl-turbo" not in manifest["tool"]["uv"]["sources"]
    lock = tomllib.loads((ROOT / "uv.lock").read_text())
    (owl,) = [row for row in lock["package"] if row["name"] == "owlrl-turbo"]
    assert owl["version"] == "0.2.0a1"
    assert owl["source"] == {"registry": "https://pypi.org/simple"}
