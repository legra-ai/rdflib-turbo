"""The sole native provider owns and measures all of its engine sources."""

import importlib.util
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_native_engine_and_bindings_are_local_workspace_crates():
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    for name, path in (
        ("pyshacl-turbo-engine", "crates/engine"),
        ("shacl-bindings", "crates/python"),
    ):
        dependency = manifest["dependencies"][name]
        assert dependency.get("path") == path
        assert not {"git", "rev", "branch", "tag"}.intersection(dependency)
        assert path in manifest["workspace"]["members"]
        assert (ROOT / path / "src/lib.rs").is_file()
    assert manifest["patch"]["crates-io"]["fancy-regex"] == {"path": "vendor/fancy-regex-0.20.0"}


def test_provider_coverage_includes_the_engine_and_binding_source_files():
    spec = importlib.util.spec_from_file_location(
        "provider_native_coverage", ROOT / "scripts/native_coverage.py"
    )
    coverage = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(coverage)
    measured = coverage.owned_sources(ROOT)
    for root in (ROOT / "crates/engine/src", ROOT / "crates/python/src"):
        assert root.is_dir()
        expected = {
            path.relative_to(ROOT).as_posix()
            for path in root.rglob("*.rs")
            if not path.name.endswith(("_tests.rs", "-tests.rs")) and path.name != "tests.rs"
        }
        assert expected
        assert expected.issubset(measured)


def test_shared_binding_crate_cannot_build_a_second_extension_or_enable_abi3():
    manifest = tomllib.loads((ROOT / "crates/python/Cargo.toml").read_text())
    assert manifest["lib"]["crate-type"] == ["rlib"]
    assert manifest["lib"]["name"] == "shacl_bindings"
    assert "standalone" not in manifest.get("features", {})
    assert not any("abi3" in str(value) for value in manifest.values())
