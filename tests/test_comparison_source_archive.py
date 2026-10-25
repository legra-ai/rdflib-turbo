"""Comparison archives retain the complete Git payload and exact backend transforms."""

import copy
import importlib
import tomllib
from pathlib import Path

import pytest


@pytest.fixture
def inputs(monkeypatch):
    monkeypatch.syspath_prepend(str(Path(__file__).resolve().parents[1] / "scripts"))
    return importlib.import_module("comparison_inputs")


@pytest.fixture
def manifests():
    fixture = Path(__file__).parent / "fixtures"
    return tuple(
        tomllib.loads((fixture / name).read_text())
        for name in ("comparison_cargo_source_1900.toml", "comparison_cargo_uv_sdist_1900.toml")
    )


def test_actual_original_uv_archive_manifest_has_only_the_expected_transform(inputs, manifests):
    original, archived = manifests
    frozen = copy.deepcopy(original)
    inputs.normalized_manifest(original, archived)
    inputs.normalized_manifest(original, original)
    assert original == frozen


@pytest.mark.parametrize(
    "path,value",
    (
        (("package", "version"), "9.0.0"),
        (("package", "readme"), "other.md"),
        (("dependencies", "spareval", "features"), []),
        (("patch", "crates-io", "spareval", "path"), "other/vendor"),
        (("features", "persistence"), []),
        (("profile", "release", "lto"), False),
        (("workspace", "members"), ["crates/python", "crates/engine"]),
        (("workspace", "members"), ["crates/engine"]),
        (("workspace", "package", "version"), "9.0.0"),
        (("workspace", "resolver"), "3"),
    ),
)
def test_backend_transform_never_permits_other_manifest_changes(inputs, manifests, path, value):
    original, archived = manifests
    target = archived
    for part in path[:-1]:
        target = target[part]
    target[path[-1]] = value
    with pytest.raises(ValueError, match="Unexpected Cargo manifest normalization"):
        inputs.normalized_manifest(original, archived)


def test_nonredundant_default_member_selection_cannot_disappear(inputs, manifests):
    original, archived = manifests
    original["workspace"]["default-members"] = ["."]
    with pytest.raises(ValueError, match="Unexpected Cargo manifest normalization"):
        inputs.normalized_manifest(original, archived)


@pytest.mark.parametrize(
    "name", ("crates/engine/src/lib.rs", "crates/python/src/lib.rs", "crates/engine/Cargo.toml")
)
def test_every_nested_crate_source_is_required(inputs, tmp_path, name):
    source, archived = tmp_path / "source", tmp_path / "archive"
    (source / name).parent.mkdir(parents=True)
    (source / name).write_text("original")
    archived.mkdir()
    with pytest.raises(ValueError, match="Missing source archive input"):
        inputs.audit_tree(source, archived, [name])
    (archived / name).parent.mkdir(parents=True)
    (archived / name).write_text("changed")
    with pytest.raises(ValueError, match="Changed source archive input"):
        inputs.audit_tree(source, archived, [name])
    (archived / name).write_bytes((source / name).read_bytes())
    assert inputs.audit_tree(source, archived, [name])[name] == inputs.sha(source / name)
