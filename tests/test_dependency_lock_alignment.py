"""Exact published dependency pins must agree with the committed Cargo graph."""

import tomllib
from pathlib import Path

from packaging.requirements import Requirement

ROOT = Path(__file__).resolve().parents[1]


def test_exact_registry_dependency_pins_match_the_cargo_lock():
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    packages = tomllib.loads((ROOT / "Cargo.lock").read_text())["package"]
    for alias, declaration in manifest["dependencies"].items():
        if isinstance(declaration, str):
            name, requirement = alias, declaration
        else:
            if "path" in declaration or "git" in declaration:
                continue
            name = declaration.get("package", alias)
            requirement = declaration.get("version", "")
        if not requirement.startswith("="):
            continue
        expected = requirement.removeprefix("=")
        resolved = [package for package in packages if package["name"] == name]
        assert any(package["version"] == expected for package in resolved), (
            f"{name}: exact manifest pin {requirement} is absent from Cargo.lock"
        )


def test_provider_test_extra_accepts_the_current_owl_maintenance_wheel():
    manifest = tomllib.loads((ROOT / "pyproject.toml").read_text())
    requirements = manifest["project"]["optional-dependencies"]["test"]
    requirement = next(
        Requirement(value) for value in requirements if Requirement(value).name == "owlrl-turbo"
    )
    for supported in ("0.2.0a1", "0.2.0a2"):
        assert requirement.specifier.contains(supported, prereleases=True)
    for unsupported in ("0.2.0a0", "0.3.0"):
        assert not requirement.specifier.contains(unsupported, prereleases=True)
