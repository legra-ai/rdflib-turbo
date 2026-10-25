"""Native coverage dependencies come from the current authoritative uv lock."""

import importlib.util
import os
import subprocess
import sys
import tomllib
from pathlib import Path
from zipfile import ZipFile

import pytest
from packaging.requirements import Requirement


def harness():
    path = Path(__file__).resolve().parents[1] / "scripts/native_coverage.py"
    spec = importlib.util.spec_from_file_location("coverage_dependencies_harness", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def yaml_wheel(directory):
    """A tiny explicit artifact exercises real uv installation without any build."""
    path = directory / "yaml_fixture-1.0-py3-none-any.whl"
    with ZipFile(path, "w") as archive:
        archive.writestr("yaml_fixture/__init__.py", "")
        archive.writestr(
            "yaml_fixture-1.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: yaml-fixture\nVersion: 1.0\n",
        )
        archive.writestr(
            "yaml_fixture-1.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        archive.writestr("yaml_fixture-1.0.dist-info/RECORD", "")
    return path


def source_project(root, project="rdflib-turbo"):
    root.mkdir()
    (root / "pyproject.toml").write_text(f'[project]\nname="{project}"\nversion="0.0.0"\n')
    (root / "uv.lock").write_text("authoritative lock")
    (root / "requirements-coverage.lock").write_text("stale==1\n")
    return {p.name: p.read_bytes() for p in root.iterdir()}


@pytest.mark.parametrize(
    ("project", "selection"),
    [("rdflib-turbo", ["--group", "coverage"]), ("pyshacl-turbo", ["--extra", "test"])],
)
def test_locked_export_precedes_isolated_import_and_preserves_source(
    tmp_path, monkeypatch, project, selection
):
    module = harness()
    root = tmp_path / "source"
    before = source_project(root, project)
    profiles, output = tmp_path / "profiles", tmp_path / "reports"
    profiles.mkdir()
    output.mkdir()
    commands = []

    def run(arguments, environment=None, capture=False):
        args = list(map(str, arguments))
        commands.append(args)
        if args[:2] == ["uv", "export"]:
            assert args[args.index("--project") + 1] == str(root.resolve())
            assert all(
                flag in args
                for flag in ("--locked", "--no-dev", "--no-emit-project", "--no-hashes")
            )
            assert args[args.index(selection[0]) : args.index(selection[0]) + 2] == selection
            if project == "rdflib-turbo":
                assert args[args.index("--no-emit-package") + 1] == "pyshacl-turbo"
                omitted = {
                    args[index + 1]
                    for index, value in enumerate(args)
                    if value == "--no-emit-package"
                }
                assert omitted == {"pyshacl-turbo", "owlrl-turbo"}
            Path(args[args.index("--output-file") + 1]).write_text("current==2\n")
        elif args[:2] == ["uv", "add"]:
            exported = Path(args[args.index("--requirements") + 1])
            assert exported.is_absolute() and exported.is_relative_to(profiles)
            assert exported.read_text() == "current==2\n"
            assert args[args.index("--project") + 1] == str(profiles / "project")
        elif args == ["uv", "--version"]:
            return "uv synthetic\n"
        return ""

    monkeypatch.setattr(module, "run", run)
    monkeypatch.setattr(module, "initialize_project", lambda *args: commands.append(["initialize"]))
    provenance = module.initialize_dependencies(
        root, profiles, output, project, sys.executable, {}, tmp_path / "yaml.whl"
    )
    assert commands[0][:2] == ["uv", "export"]
    addition = next(command for command in commands if command[:2] == ["uv", "add"])
    assert addition[-1] == str(tmp_path / "yaml.whl")
    sync = next(command for command in commands if command[:2] == ["uv", "sync"])
    assert "--locked" in sync and "--no-build" in sync
    assert [p.name for p in root.iterdir()] == list(before)
    assert {p.name: p.read_bytes() for p in root.iterdir()} == before
    assert provenance["source_uv_lock_sha256"] == module.digest(root / "uv.lock")
    assert provenance["export_sha256"] == module.digest(output / "dependencies/requirements.lock")
    assert provenance["uv_version"] == "uv synthetic"
    module.verify_dependency_inputs(root, profiles, output, provenance)
    (output / "dependencies/requirements.lock").write_text("tampered export")
    with pytest.raises(RuntimeError, match="changed"):
        module.verify_dependency_inputs(root, profiles, output, provenance)
    (output / "dependencies/requirements.lock").write_text("current==2\n")
    (root / "uv.lock").write_text("changed")
    with pytest.raises(RuntimeError, match="changed"):
        module.verify_dependency_inputs(root, profiles, output, provenance)


def test_export_failure_never_initializes_or_installs(tmp_path, monkeypatch):
    module = harness()
    root = tmp_path / "source"
    before = source_project(root)

    def failed_export(*args, **kwargs):
        raise RuntimeError("locked export failed")

    monkeypatch.setattr(module, "run", failed_export)
    monkeypatch.setattr(
        module, "initialize_project", lambda *args: pytest.fail("initialized after export failure")
    )
    with pytest.raises(RuntimeError, match="locked export failed"):
        module.initialize_dependencies(
            root,
            tmp_path / "profiles",
            tmp_path / "reports",
            "rdflib-turbo",
            sys.executable,
            {},
            tmp_path / "yaml.whl",
        )
    assert {p.name: p.read_bytes() for p in root.iterdir()} == before


def test_environment_locks_are_preserved_per_stage(tmp_path):
    module = harness()
    project = tmp_path / "project"
    project.mkdir()
    output = tmp_path / "reports"
    (output / "dependencies").mkdir(parents=True)
    (project / "uv.lock").write_text("python environment")
    python = module.record_environment_lock(project, output, "python")
    (project / "uv.lock").write_text("companion environment")
    companion = module.record_environment_lock(project, output, "owlrl")
    assert python != companion
    assert (output / "dependencies/python-uv.lock").read_text() == "python environment"
    assert (output / "dependencies/owlrl-uv.lock").read_text() == "companion environment"


@pytest.mark.skipif(
    sys.platform not in {"darwin", "linux"},
    reason="Native coverage runner requires Linux/macOS posix_spawn",
)
def test_real_locked_export_ignores_stale_snapshot_and_fails_stale_uv_lock(tmp_path, monkeypatch):
    module = harness()
    root = tmp_path / "source"
    root.mkdir()
    manifest = '[project]\nname="rdflib-turbo"\nversion="0.0.0"\nrequires-python=">=3.11"\n[dependency-groups]\ncoverage=["packaging==26.3"]\n'
    (root / "pyproject.toml").write_text(manifest)
    environment = dict(os.environ)
    for name in ("VIRTUAL_ENV", "UV_PYTHON", "UV_PROJECT", "UV_PROJECT_ENVIRONMENT"):
        environment.pop(name, None)
    subprocess.run(
        ["uv", "lock", "--project", str(root), "--python", sys.executable],
        env=environment,
        check=True,
    )
    (root / "requirements-coverage.lock").write_text("packaging==25.0\n")
    before = {p.name: p.read_bytes() for p in root.iterdir()}
    monkeypatch.chdir(tmp_path)
    profiles, output = tmp_path / "profiles", tmp_path / "reports"
    profiles.mkdir()
    output.mkdir()
    provenance = module.initialize_dependencies(
        root, profiles, output, "rdflib-turbo", sys.executable, environment, yaml_wheel(tmp_path)
    )
    installed = subprocess.check_output(
        [
            "uv",
            "run",
            "--project",
            str(profiles / "project"),
            "--locked",
            "--no-sync",
            "python",
            "-I",
            "-c",
            "import packaging; print(packaging.__version__)",
        ],
        env=environment,
        text=True,
    )
    assert installed.strip() == "26.3"
    assert {p.name: p.read_bytes() for p in root.iterdir()} == before
    module.verify_dependency_inputs(root, profiles, output, provenance)
    (root / "pyproject.toml").write_text(manifest.replace("26.3", "25.0"))
    with pytest.raises(RuntimeError, match="Command failed"):
        module.initialize_dependencies(
            root,
            tmp_path / "failed-profiles",
            tmp_path / "failed-reports",
            "rdflib-turbo",
            sys.executable,
            environment,
            yaml_wheel(tmp_path),
        )
    assert not (tmp_path / "failed-profiles/project").exists()


def test_project_identity_mismatch_fails_before_any_command(tmp_path, monkeypatch):
    module = harness()
    root = tmp_path / "source"
    source_project(root, "another-project")
    monkeypatch.setattr(
        module, "run", lambda *args, **kwargs: pytest.fail("ran command for wrong project")
    )
    with pytest.raises(ValueError, match="identity"):
        module.initialize_dependencies(
            root,
            tmp_path / "profiles",
            tmp_path / "reports",
            "rdflib-turbo",
            sys.executable,
            {},
            tmp_path / "yaml.whl",
        )


def test_source_distribution_uses_uv_lock_without_legacy_dependency_snapshots():
    import tomllib

    root = Path(__file__).resolve().parents[1]
    manifest = tomllib.loads((root / "pyproject.toml").read_text())
    includes = {entry["path"] for entry in manifest["tool"]["maturin"]["include"]}
    assert "uv.lock" in includes
    for name in ("requirements-coverage.in", "requirements-coverage.lock"):
        assert not (root / name).exists()
        assert name not in includes
    assert "scripts/**/*" in includes


def test_coverage_companion_uses_the_locked_provider_facade():
    root = Path(__file__).resolve().parents[1]
    cargo = tomllib.loads((root / "Cargo.toml").read_text())
    project = tomllib.loads((root / "pyproject.toml").read_text())
    lock = tomllib.loads((root / "uv.lock").read_text())
    engine = cargo["dependencies"]["pyshacl-turbo-engine"]
    companion = project["tool"]["uv"]["sources"]["pyshacl-turbo"]
    assert engine == {"path": "crates/engine"}
    assert cargo["patch"]["crates-io"]["fancy-regex"] == {"path": "vendor/fancy-regex-0.20.0"}
    assert set(companion) == {"path", "editable"}
    assert companion["editable"] is True
    package = next(item for item in lock["package"] if item["name"] == "pyshacl-turbo")
    assert package["source"] == {"editable": companion["path"]}
    requirement = next(
        Requirement(value)
        for value in project["dependency-groups"]["coverage"]
        if Requirement(value).name == "pyshacl-turbo"
    )
    assert "test" in requirement.extras
