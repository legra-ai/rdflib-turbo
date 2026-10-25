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


def test_profile_rejects_installed_turbo_before_source_finder(monkeypatch):
    root = Path(__file__).resolve().parents[1]
    spec = importlib.util.spec_from_file_location(
        "memory_fixture_installed_guard", root / "scripts/canonical_memory_fixture.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    monkeypatch.setattr(module.importlib.metadata, "distribution", lambda name: object())
    with pytest.raises(RuntimeError, match="forbids installed turbo distribution"):
        module.assert_no_installed_turbo()


def test_export_context_has_independent_declaration_authority(profiler, monkeypatch, tmp_path):
    import ci_facade

    monkeypatch.setattr(
        ci_facade,
        "checkout",
        lambda selected, destination: pytest.fail(
            "dependency role must be independently checked before checkout"
        ),
    )
    monkeypatch.setattr(
        ci_facade,
        "policy",
        lambda: {"facade": {"revision": "b" * 40}, "owlrl": {"revision": "c" * 40}},
    )
    with pytest.raises(ValueError, match="independent declaration"):
        profiler.source_inputs(tmp_path / "sources")


def test_public_source_layout_comes_from_declared_build_inputs(tmp_path):
    root = Path(__file__).resolve().parents[1]
    spec = importlib.util.spec_from_file_location(
        "memory_fixture_layout", root / "scripts/canonical_memory_fixture.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    (tmp_path / "src/owlrl_turbo").mkdir(parents=True)
    (tmp_path / "pyproject.toml").write_text(
        '[project]\nname="owlrl-turbo"\n[tool.hatch.build.targets.wheel]\npackages=["src/owlrl_turbo"]\n'
    )
    assert module.source_package_root(tmp_path, "owlrl-turbo") == tmp_path / "src"
    finder = module.SourceFinder({"owlrl-turbo": "0.1.0a3"})
    assert list(finder.find_distributions()) == []


@pytest.mark.parametrize("preloaded", [False, True], ids=["cold", "preloaded-family"])
def test_semantic_audit_runs_after_measured_release_with_actual_package_init(
    tmp_path, monkeypatch, preloaded
):
    import sys
    from types import SimpleNamespace

    root = Path(__file__).resolve().parents[1]
    spec = importlib.util.spec_from_file_location(
        "memory_fixture_boundaries", root / "scripts/canonical_memory_fixture.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    if preloaded:
        for project in module.PROJECTS:
            name = project.replace("-", "_")
            for loaded in (name, name + ".preexisting"):
                monkeypatch.setitem(sys.modules, loaded, SimpleNamespace(loaded=loaded))
    inputs = {}
    for name in module.PROJECTS:
        source = tmp_path / name
        package = source / "python" / name.replace("-", "_")
        package.mkdir(parents=True)
        (package / "__init__.py").write_text("initialized = True\n")
        tool = (
            '[tool.maturin]\npython-source="python"\n'
            if name == "rdflib-turbo"
            else f'[tool.hatch.build.targets.wheel]\npackages=["python/{name.replace("-", "_")}"]\n'
        )
        (source / "pyproject.toml").write_text(
            f'[project]\nname="{name}"\nversion="1.0.0"\n' + tool
        )
        inputs[name] = {"source": str(source)}
    inputs["declaration"] = {"source": str(tmp_path)}
    monkeypatch.setattr(module, "assert_no_installed_turbo", lambda: None)
    events = []
    native = SimpleNamespace()

    def pipeline(received, phase, *args, observe):
        assert received is native
        package = sys.modules["rdflib_turbo"]
        assert package.initialized and package._native is native
        events.append("semantic" if observe else "measurement")
        phase("validation_public_report", 1)
        return {"native_origin": "compiled-source-test-binary"}

    monkeypatch.setattr(module, "pipeline", pipeline)
    families = {project.replace("-", "_") for project in module.PROJECTS}
    before = {
        name: loaded for name, loaded in sys.modules.items() if name.split(".")[0] in families
    }
    with monkeypatch.context() as isolated:
        for name in before:
            isolated.delitem(sys.modules, name)
        module.execute(
            native, lambda name, count: events.append(name), inputs, "data", "shapes", "rdfs"
        )
        assert events == ["measurement", "validation_public_report", "released", "semantic"]
        assert "rdflib_turbo" not in sys.modules
    after = {name: loaded for name, loaded in sys.modules.items() if name.split(".")[0] in families}
    assert after.keys() == before.keys()
    assert all(after[name] is loaded for name, loaded in before.items())


def test_opt_in_allocator_fixture_is_not_a_production_coverage_source(profiler):
    import native_coverage

    root = Path(__file__).resolve().parents[1]
    fixtures = list((root / "src").glob("allocation_canonical_profile*.rs"))
    assert len(fixtures) == 1
    assert str(fixtures[0].relative_to(root)) not in native_coverage.owned_sources(root)


def test_report_receipt_preserves_equal_lexical_literals_datatypes_and_languages():
    from rdflib import Graph, Literal, URIRef
    from rdflib.namespace import XSD

    root = Path(__file__).resolve().parents[1]
    spec = importlib.util.spec_from_file_location(
        "memory_full_report_terms", root / "scripts/canonical_memory_fixture.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    graph = Graph()
    values = [
        Literal("same"),
        Literal("same", datatype=XSD.string),
        Literal("same", lang="en"),
        Literal("same", lang="fr"),
    ]
    for value in values:
        graph.add((URIRef("urn:report"), URIRef("urn:value"), value))
    observed = {row[2] for row in module.report_terms(graph)}
    assert observed == {
        ("Literal", "same", None, None),
        ("Literal", "same", str(XSD.string), None),
        ("Literal", "same", None, "en"),
        ("Literal", "same", None, "fr"),
    }
