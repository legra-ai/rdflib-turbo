"""Exact published dependency pins must agree with the committed Cargo graph."""

import tomllib
from pathlib import Path

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
