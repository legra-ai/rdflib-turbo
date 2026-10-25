"""Requested-heap profiles must identify their actual storage owner explicitly."""

import importlib.util
from pathlib import Path

import pytest


@pytest.fixture
def profiler(monkeypatch):
    root = Path(__file__).resolve().parents[1]
    monkeypatch.syspath_prepend(str(root / "scripts"))
    spec = importlib.util.spec_from_file_location(
        "canonical_memory_driver", root / "scripts/profile_memory.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_profile_rejects_unspecified_owner_before_dependency_setup(profiler, monkeypatch, tmp_path):
    monkeypatch.setattr(
        profiler.sys,
        "argv",
        [
            "profile_memory",
            "--data",
            "data.ttl",
            "--shapes",
            "shapes.ttl",
            "--output",
            str(tmp_path / "report.json"),
            "--yaml-authority",
            "yaml.json",
        ],
    )
    monkeypatch.setattr(
        profiler,
        "locked_python_environment",
        lambda *args: pytest.fail("owner must be selected before any setup"),
    )
    with pytest.raises(SystemExit) as failure:
        profiler.main()
    assert failure.value.code == 2


def test_canonical_profile_selects_the_public_store_owner(profiler):
    assert (
        profiler.profile_case("canonical-store")
        == "allocation_tests::canonical::profile_canonical_store_pipeline_allocations"
    )
    assert (
        profiler.profile_case("generalized-dataset")
        == "allocation_tests::profile_pipeline_allocations"
    )


def test_source_metadata_uses_exact_manifests_and_restores_finders(tmp_path, monkeypatch):
    import importlib.metadata
    import sys

    root = Path(__file__).resolve().parents[1]
    spec = importlib.util.spec_from_file_location(
        "memory_fixture", root / "scripts/canonical_memory_fixture.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    sources = {}
    for name, version in (
        ("rdflib-turbo", "0.2.0a19"),
        ("pyshacl-turbo", "0.7.0"),
        ("owlrl-turbo", "0.1.0a3"),
    ):
        path = tmp_path / name
        path.mkdir()
        (path / "pyproject.toml").write_text(f'[project]\nname="{name}"\nversion="{version}"\n')
        sources[name] = path
    monkeypatch.setattr(module, "assert_no_installed_turbo", lambda: None)
    before = tuple(sys.meta_path)
    with module.source_metadata(sources) as receipt:
        assert importlib.metadata.version("rdflib-turbo") == "0.2.0a19"
        assert receipt["kind"] == "source-test-metadata"
        assert len(receipt["projects"]) == 3
    assert tuple(sys.meta_path) == before


def test_source_metadata_cannot_fabricate_wheel_or_native_file_authority():
    root = Path(__file__).resolve().parents[1]
    spec = importlib.util.spec_from_file_location(
        "memory_fixture_guards", root / "scripts/canonical_memory_fixture.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    distribution = module.SourceDistribution("rdflib-turbo", "0.2.0a19")
    for name in ("WHEEL", "RECORD"):
        with pytest.raises(RuntimeError, match="cannot supply wheel authority"):
            distribution.read_text(name)
    with pytest.raises(RuntimeError, match="cannot locate installed files"):
        distribution.locate_file("rdflib_turbo/_native.so")
