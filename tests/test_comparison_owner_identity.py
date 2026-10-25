"""Current comparison identity is owned by the exact archived engine and all seven forks."""

import copy
import importlib.util
from pathlib import Path

import pytest


def load(name):
    path = Path(__file__).resolve().parents[1] / "scripts" / (name + ".py")
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.fixture
def owner(tmp_path):
    path = Path(__file__).with_name("comparison_owner_fixture.py")
    spec = importlib.util.spec_from_file_location("owner_fixture", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.make_owner(tmp_path)


def test_archive_local_owner_matches_the_proven_independent_vector(owner):
    result = load("comparison_identity").derive(owner["source"], owner["metadata"])
    assert result["engine_source"] == "crates/engine"
    assert result["engine_source_count"] == 2
    assert len(result["dependency_fingerprints"]) == 7
    assert "fancy-regex" in result["dependency_fingerprints"]
    assert "engine_revision" not in result
    assert (
        result["engine_fingerprint"]
        == "94c963638fea96add80218f718a80daea1142525a19e3e6adeeedcecb245481b"
    )


@pytest.mark.parametrize("role", ("engine", "fancy-regex"))
@pytest.mark.parametrize("tamper", ("external", "different-owned-path", "missing"))
def test_resolved_packages_must_match_the_declared_archive_owner(owner, role, tamper):
    metadata = copy.deepcopy(owner["metadata"])
    package = next(p for p in metadata["packages"] if p["id"] == role)
    if tamper == "external":
        package["source"] = "git+https://example.invalid/substitute#" + "a" * 40
    elif tamper == "different-owned-path":
        package["manifest_path"] = str(owner["source"] / "vendor/substitute/Cargo.toml")
    else:
        metadata["packages"].remove(package)
    with pytest.raises(ValueError):
        load("comparison_identity").derive(owner["source"], metadata)


def test_declared_engine_path_cannot_disagree_with_the_resolved_package(owner):
    manifest = owner["source"] / "Cargo.toml"
    manifest.write_text(manifest.read_text().replace('path="crates/engine"', 'path="other-engine"'))
    with pytest.raises(ValueError):
        load("comparison_identity").derive(owner["source"], owner["metadata"])


def test_complete_engine_implementation_is_mandatory(owner):
    (owner["source"] / "crates/engine/src/omitted.rs").write_text("pub fn omitted() {}\n")
    with pytest.raises(ValueError, match="complete engine"):
        load("comparison_identity").derive(owner["source"], owner["metadata"])


def test_seventh_fork_bytes_and_semantic_features_are_bound(owner):
    helper = load("comparison_identity")
    original = helper.derive(owner["source"], owner["metadata"])
    path = owner["source"] / "vendor/fancy-regex-0.1.0/src/lib.rs"
    raw = path.read_bytes()
    path.write_bytes(raw + b"// changed matcher\n")
    assert (
        helper.derive(owner["source"], owner["metadata"])["engine_fingerprint"]
        != original["engine_fingerprint"]
    )
    path.write_bytes(raw)
    node = next(
        node for node in owner["metadata"]["resolve"]["nodes"] if node["id"] == "fancy-regex"
    )
    node["features"].reverse()
    assert helper.derive(owner["source"], owner["metadata"]) == original
    node["features"].append("future-semantic")
    assert (
        helper.derive(owner["source"], owner["metadata"])["engine_fingerprint"]
        != original["engine_fingerprint"]
    )
