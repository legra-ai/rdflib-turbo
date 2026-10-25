"""Test-binary-only public pipeline; never an installed-wheel authority."""

import decimal
import importlib
import importlib.metadata
import sys
import tomllib
from contextlib import contextmanager
from pathlib import Path

PROJECTS = {"rdflib-turbo", "pyshacl-turbo", "owlrl-turbo"}


def assert_no_installed_turbo():
    for name in PROJECTS:
        try:
            importlib.metadata.distribution(name)
        except importlib.metadata.PackageNotFoundError:
            continue
        raise RuntimeError("Source profile forbids installed turbo distribution: " + name)


class SourceDistribution(importlib.metadata.Distribution):
    def __init__(self, name, version):
        self.name_version = (name, version)

    def read_text(self, filename):
        if filename == "METADATA":
            name, version = self.name_version
            return f"Metadata-Version: 2.1\nName: {name}\nVersion: {version}\n"
        raise RuntimeError("Source-test metadata cannot supply wheel authority: " + filename)

    def locate_file(self, path):
        raise RuntimeError("Source-test metadata cannot locate installed files")


class SourceFinder(importlib.metadata.DistributionFinder):
    def __init__(self, projects):
        self.projects = projects

    def find_spec(self, fullname, path=None, target=None):
        return None

    def find_distributions(self, context=importlib.metadata.DistributionFinder.Context()):
        normalized = context.name.lower().replace("_", "-") if context.name else None
        return iter(
            SourceDistribution(name, version)
            for name, version in self.projects.items()
            if normalized == name
        )


@contextmanager
def source_metadata(sources):
    assert_no_installed_turbo()
    if set(sources) != PROJECTS:
        raise RuntimeError("Exact three source projects are required")
    projects = {}
    for name, source in sources.items():
        manifest = tomllib.loads((source / "pyproject.toml").read_text())["project"]
        if manifest["name"] != name or not isinstance(manifest["version"], str):
            raise RuntimeError("Source project metadata identity differs")
        projects[name] = manifest["version"]
    finder = SourceFinder(projects)
    before = list(sys.meta_path)
    sys.meta_path.insert(0, finder)
    try:
        yield {"kind": "source-test-metadata", "projects": projects}
    finally:
        sys.meta_path[:] = before


def full_term_key(term):
    from rdflib import Literal

    return (
        type(term).__name__,
        str(term),
        str(term.datatype) if isinstance(term, Literal) and term.datatype is not None else None,
        term.language if isinstance(term, Literal) else None,
    )


def facts(data):
    default = data.default_graph.identifier
    return {
        (*map(full_term_key, (s, p, o)), None if graph == default else full_term_key(graph))
        for s, p, o, graph in data.quads((None,) * 4)
    }


def report_terms(graph):
    return sorted((tuple(map(full_term_key, triple)) for triple in graph), key=repr)


def source_package_root(source, name):
    manifest = tomllib.loads((source / "pyproject.toml").read_text())
    tool = manifest["tool"]
    package = name.replace("-", "_")
    if name == "rdflib-turbo":
        root = source / tool["maturin"]["python-source"]
    else:
        declared = tool["hatch"]["build"]["targets"]["wheel"]["packages"]
        if len(declared) != 1 or Path(declared[0]).name != package:
            raise RuntimeError("Source package differs from its declared wheel layout")
        root = source / Path(declared[0]).parent
    if not (root / package).resolve(strict=True).is_relative_to(source.resolve()):
        raise RuntimeError("Declared source package escapes its source authority")
    return root


def execute(native, phase, inputs, data_path, shapes_path, mode):
    """Run once in an isolated exact Rust test child, keeping only owner IDs."""
    if any(name.split(".")[0] in {p.replace("-", "_") for p in PROJECTS} for name in sys.modules):
        raise RuntimeError("Turbo modules were loaded before source-module injection")
    if getattr(native, "__file__", None) is not None:
        raise RuntimeError("Expected compiled test-binary module, not an installed DSO")
    if set(inputs) != PROJECTS | {"declaration"}:
        raise RuntimeError("Expected separate source and export-declaration authorities")
    sources = {name: Path(inputs[name]["source"]) for name in PROJECTS}
    old_path = list(sys.path)
    with source_metadata(sources) as metadata:
        sys.path[:0] = [str(source_package_root(source, name)) for name, source in sources.items()]
        sys.modules["rdflib_turbo._native"] = native
        try:
            package = importlib.import_module("rdflib_turbo")
            package._native = native
            result = pipeline(native, phase, data_path, shapes_path, mode, sources, observe=False)
            # Observe teardown before any semantic materialization. The normal
            # bounded compile cache and source modules remain naturally loaded.
            phase("released", 0)
            audit = pipeline(
                native, lambda *_: None, data_path, shapes_path, mode, sources, observe=True
            )
            result["semantic_run"] = audit
            result["measurement_run"] = "public-pipeline-before-independent-semantic-run"
            result["released_scope"] = (
                "explicit-close-and-local-handles-drop-with-normal-compile-cache"
            )
            result["metadata"] = metadata
        finally:
            for name in list(sys.modules):
                if name.split(".")[0] in {p.replace("-", "_") for p in PROJECTS}:
                    del sys.modules[name]
            sys.path[:] = old_path
        return result


def pipeline(native, phase, data_path, shapes_path, mode, sources, *, observe):
    import owlrl_turbo
    import pyshacl_turbo
    import rdflib
    import rdflib_turbo

    __import__("owlrl")
    __import__("pyshacl")

    assert rdflib_turbo.Dataset is rdflib_turbo.StoreDataset
    assert rdflib_turbo._native is native
    assert pyshacl_turbo._native._bindings is native.shacl
    for module in (rdflib_turbo, owlrl_turbo, pyshacl_turbo):
        project = module.__name__.replace("_", "-")
        assert (
            Path(module.__file__)
            .resolve()
            .is_relative_to(source_package_root(sources[project], project))
        )
    text, shape_text = Path(data_path).read_text(), Path(shapes_path).read_text()
    rdflib.NORMALIZE_LITERALS = False
    shape_graph = rdflib.Graph().parse(data=shape_text, format="turtle")
    with decimal.localcontext() as context:
        context.clear_flags()
        phase("setup", 0)
        data = rdflib_turbo.StoreDataset()

        identity = owner_ids(data)
        assert type(data.store.backend._native) is native.NativeStore
        data.parse(data=text, format="turtle")
        phase("parse", len(data))
        assert owner_ids(data) == identity
        assert owlrl_turbo.expand(data, inference=mode, inplace=True) is data
        phase("inference", len(data))
        assert owner_ids(data) == identity
        shapes = pyshacl_turbo.CompiledShapes(shape_graph)
        phase("compile_shapes", len(data))
        report = shapes.validate(data, inference="none")
        phase("validation_public_report", len(data))
        assert owner_ids(data) == identity
        flags = sorted(signal.__name__ for signal, active in context.flags.items() if active)
    result = {
        "owner_ids": identity,
        "native_origin": "compiled-source-test-binary",
        "compile_cache_entries": pyshacl_turbo._compile_native.cache_info().currsize,
    }
    if observe:
        observation = {
            "physical": facts(data),
            "contexts": {str(g.identifier) for g in data.contexts()},
            "report": report,
            "flags": flags,
        }
        result["observation"] = validate_observation(data, observation, text, shape_text, mode)

        class RejectedProfileTransaction(Exception):
            pass

        try:
            with data.transaction() as pending:
                pending.add(
                    (
                        rdflib.URIRef("urn:profile:rejected"),
                        rdflib.URIRef("urn:profile:p"),
                        rdflib.Literal("rejected"),
                    )
                )
                raise RejectedProfileTransaction
        except RejectedProfileTransaction:
            pass
        assert facts(data) == observation["physical"]
        assert owner_ids(data) == identity
        result["rollback_complete_physical_facts_equal"] = True
    data.close()
    del report, shapes, data, shape_graph
    return result


def owner_ids(data):
    return tuple(map(id, (data, data.store, data.store.backend, data.store.backend._native)))


def validate_observation(data, observation, text, shape_text, mode):
    import owlrl
    import pyshacl
    import rdflib
    from rdflib.compare import isomorphic

    original = rdflib.Dataset()
    original.parse(data=text, format="turtle")
    before = facts(original)
    shapes = rdflib.Graph().parse(data=shape_text, format="turtle")
    with decimal.localcontext() as context:
        context.clear_flags()
        semantics = {
            "rdfs": owlrl.RDFS_Semantics,
            "owlrl": owlrl.OWLRL_Semantics,
            "both": owlrl.RDFS_OWLRL_Semantics,
        }[mode]
        # The public Dataset profile puts consequences in its explicit inference context.
        owlrl.DeductiveClosure(semantics).expand(
            original, destination=original.graph(rdflib.URIRef("urn:owlrl-turbo:inference"))
        )
        expected = pyshacl.validate(original, shacl_graph=shapes, inference="none")
        flags = sorted(signal.__name__ for signal, active in context.flags.items() if active)
    assert facts(data) == observation["physical"] == facts(original)
    assert before.issubset(facts(data))
    assert observation["flags"] == flags
    actual = observation["report"]
    assert actual[0] == expected[0] and actual[2] == expected[2]
    assert isomorphic(actual[1], expected[1])
    assert {str(g.identifier) for g in original.contexts()} == observation["contexts"]
    return {
        "physical_quads": sorted(observation["physical"], key=repr),
        "input_physical_quads": sorted(before, key=repr),
        "contexts": sorted(observation["contexts"]),
        "flags": flags,
        "conforms": actual[0],
        "report_text": actual[2],
        "report_triples": report_terms(actual[1]),
        "original_report_triples": report_terms(expected[1]),
    }
