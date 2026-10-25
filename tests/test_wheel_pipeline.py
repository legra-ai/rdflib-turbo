"""Every release wheel is selected and exercised only on its own interpreter."""

import importlib.util
import subprocess
import sys
import types
from pathlib import Path
from zipfile import ZipFile

import pytest

ROOT = Path(__file__).resolve().parents[1]


def test_verifier_loads_with_python_isolated_mode():
    result = subprocess.run(
        [
            sys.executable,
            "-I",
            "-c",
            "import runpy,sys; runpy.run_path(sys.argv[1], run_name='wheel_verifier_probe')",
            str(ROOT / "scripts/test_wheel.py"),
        ],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr


def driver(monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / "scripts"))
    spec = importlib.util.spec_from_file_location("wheel_driver", ROOT / "scripts/test_wheel.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.yaml_checker = lambda: types.SimpleNamespace(
        verify=lambda owner, authority: {
            "wheel": Path(authority).parent / "pyyaml-0.0.0-py3-none-any.whl",
            "record": {},
        }
    )
    return module


def wheels(directory):
    paths = {}
    for version in ("3.11", "3.12", "3.13", "3.14", "3.15"):
        tag = "cp" + version.replace(".", "")
        path = directory / f"rdflib_turbo-0.2.0a1-{tag}-{tag}-win_amd64.whl"
        path.touch()
        paths[version] = path
    return paths


def test_selects_one_native_wheel_for_each_matching_interpreter(tmp_path, monkeypatch):
    expected = wheels(tmp_path)
    assert driver(monkeypatch).select_wheels(tmp_path, "0.2.0a1") == expected


@pytest.mark.parametrize("version", ("3.11", "3.12", "3.13", "3.14", "3.15"))
def test_missing_python_wheel_is_a_hard_error(tmp_path, monkeypatch, version):
    paths = wheels(tmp_path)
    paths[version].unlink()
    with pytest.raises(ValueError):
        driver(monkeypatch).select_wheels(tmp_path, "0.2.0a1")


@pytest.mark.parametrize("abi", ("abi3", "cp312", "cp313t"))
def test_wrong_or_stable_abi_is_rejected(tmp_path, monkeypatch, abi):
    paths = wheels(tmp_path)
    paths["3.13"].rename(tmp_path / f"rdflib_turbo-0.2.0a1-cp313-{abi}-win_amd64.whl")
    with pytest.raises(ValueError):
        driver(monkeypatch).select_wheels(tmp_path, "0.2.0a1")


def test_extra_native_wheel_is_a_hard_error(tmp_path, monkeypatch):
    wheels(tmp_path)
    (tmp_path / "rdflib_turbo-0.2.0a1-cp316-cp316-win_amd64.whl").touch()
    with pytest.raises(ValueError):
        driver(monkeypatch).select_wheels(tmp_path, "0.2.0a1")


def test_mixed_platform_wheels_are_rejected(tmp_path, monkeypatch):
    paths = wheels(tmp_path)
    paths["3.13"].rename(tmp_path / "rdflib_turbo-0.2.0a1-cp313-cp313-macosx_11_0_arm64.whl")
    with pytest.raises(ValueError):
        driver(monkeypatch).select_wheels(tmp_path, "0.2.0a1")


def test_installed_matrix_tests_every_matching_pair(tmp_path, monkeypatch):
    module = driver(monkeypatch)
    expected = wheels(tmp_path)
    called = []
    monkeypatch.setattr(
        module,
        "test_installed_wheel",
        lambda wheel, version, facade, owl, yaml: called.append((wheel, version, facade, owl)),
    )
    facade = facade_wheel(tmp_path)
    owl = reasoning_wheel(tmp_path)
    monkeypatch.setattr(module, "prefetch_test_dependencies", lambda: None)
    module.test_matrix(expected, facade, owl, tmp_path / "yaml-inputs.json")
    assert called == [
        (expected[version], version, facade, owl)
        for version in ("3.11", "3.12", "3.13", "3.14", "3.15")
    ]


@pytest.mark.parametrize("missing", ("LICENSE-APACHE", "LICENSE-LEVELDB", "LICENSE-LZ4"))
def test_every_wheel_requires_all_bundled_native_licenses(tmp_path, monkeypatch, missing):
    wheel = tmp_path / "native.whl"
    with ZipFile(wheel, "w") as archive:
        for name in ("LICENSE-APACHE", "LICENSE-LEVELDB", "LICENSE-LZ4"):
            if name != missing:
                relative = f"vendor/oxrocksdb-sys-0.5.11-notices/{name}"
                archive.writestr(
                    f"example.dist-info/licenses/{relative}", (ROOT / relative).read_bytes()
                )
    with pytest.raises(RuntimeError, match=missing):
        driver(monkeypatch).verify_licenses(wheel)


def installed_fixture(tmp_path, monkeypatch):
    environment = tmp_path / "venv"
    package = environment / "site-packages/rdflib_turbo"
    package.mkdir(parents=True)
    (package / "__init__.py").write_bytes(b"package")
    (package / "_native.so").write_bytes(b"native")
    wheel = tmp_path / "native.whl"
    with ZipFile(wheel, "w") as archive:
        archive.writestr("rdflib_turbo/__init__.py", b"package")
        archive.writestr("rdflib_turbo/_native.so", b"native")
    for name, path in (
        ("rdflib_turbo", package / "__init__.py"),
        ("rdflib_turbo._native", package / "_native.so"),
    ):
        module = types.ModuleType(name)
        module.__file__ = str(path)
        monkeypatch.setitem(sys.modules, name, module)
    return wheel, environment, package


def test_loaded_wheel_payload_is_verified(tmp_path, monkeypatch):
    wheel, environment, _ = installed_fixture(tmp_path, monkeypatch)
    version = f"{sys.version_info.major}.{sys.version_info.minor}"
    driver(monkeypatch).verify_loaded_package(wheel, version, environment)


@pytest.mark.parametrize("file", ("__init__.py", "_native.so"))
def test_changed_installed_payload_is_rejected(tmp_path, monkeypatch, file):
    wheel, environment, package = installed_fixture(tmp_path, monkeypatch)
    (package / file).write_bytes(b"different")
    version = f"{sys.version_info.major}.{sys.version_info.minor}"
    with pytest.raises(RuntimeError, match="payload"):
        driver(monkeypatch).verify_loaded_package(wheel, version, environment)


def test_source_import_is_rejected(tmp_path, monkeypatch):
    wheel, _, package = installed_fixture(tmp_path, monkeypatch)
    version = f"{sys.version_info.major}.{sys.version_info.minor}"
    with pytest.raises(RuntimeError, match="outside"):
        driver(monkeypatch).verify_loaded_package(wheel, version, package / "unrelated")


def test_wrong_interpreter_is_rejected(tmp_path, monkeypatch):
    wheel, environment, _ = installed_fixture(tmp_path, monkeypatch)
    with pytest.raises(RuntimeError, match="interpreter"):
        driver(monkeypatch).verify_loaded_package(wheel, "3.99", environment)


def metadata_wheel(path):
    with ZipFile(path, "w") as archive:
        archive.writestr(
            "rdflib_turbo-0.0.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: rdflib-turbo\nVersion: 0.0.0\n",
        )
    return path


def facade_wheel(directory):
    path = directory / "pyshacl_turbo-0.0.0-py3-none-any.whl"
    with ZipFile(path, "w") as archive:
        archive.writestr(
            "pyshacl_turbo-0.0.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: pyshacl-turbo\nVersion: 0.0.0\n",
        )
        archive.writestr(
            "pyshacl_turbo-0.0.0.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        archive.writestr("pyshacl_turbo/__init__.py", b"")
        archive.writestr("pyshacl_turbo/_native.py", b"shared binding")
    return path


def reasoning_wheel(directory):
    path = directory / "owlrl_turbo-0.0.0-py3-none-any.whl"
    with ZipFile(path, "w") as archive:
        archive.writestr(
            "owlrl_turbo-0.0.0.dist-info/METADATA", "Name: owlrl-turbo\nVersion: 0.0.0\n"
        )
        archive.writestr(
            "owlrl_turbo-0.0.0.dist-info/WHEEL", "Root-Is-Purelib: true\nTag: py3-none-any\n"
        )
        archive.writestr("owlrl_turbo/__init__.py", b"original reasoning")
    return path


def test_wheel_environment_imports_locked_companion_before_artifact(tmp_path, monkeypatch):
    module = driver(monkeypatch)
    commands = []
    monkeypatch.setattr(module, "verify_licenses", lambda wheel: None)
    monkeypatch.setattr(
        module.subprocess, "run", lambda command, **kwargs: commands.append(command)
    )
    wheel = metadata_wheel(tmp_path / "candidate.whl")
    facade = facade_wheel(tmp_path)
    module.test_installed_wheel(
        wheel, "3.13", facade, reasoning_wheel(tmp_path), tmp_path / "yaml-inputs.json"
    )
    export = next(command for command in commands if command[:2] == ["uv", "export"])
    assert "--locked" in export and "--no-emit-project" in export
    assert export[export.index("--group") + 1] == "coverage"
    assert export[export.index("--no-emit-package") + 1] == "pyshacl-turbo"
    dependencies = next(command for command in commands if "--requirements" in command)
    artifact = next(command for command in commands if str(wheel) in command)
    assert commands.index(export) < commands.index(dependencies) < commands.index(artifact)
    assert "--extra" not in dependencies
    sync = next(command for command in commands if command[:2] == ["uv", "sync"])
    assert "--locked" in sync
    assert "--no-build" in sync
    assert any(str(facade) in command and command[:2] == ["uv", "add"] for command in commands)
    assert not any(
        "--rev" in command or "--config-settings-package" in command for command in commands
    )


def test_dependency_export_failure_stops_wheel_installation(tmp_path, monkeypatch):
    module = driver(monkeypatch)
    commands = []
    monkeypatch.setattr(module, "verify_licenses", lambda wheel: None)

    def fail(command, **kwargs):
        commands.append(command)
        if command[:2] == ["uv", "export"]:
            raise subprocess.CalledProcessError(9, command)

    monkeypatch.setattr(module.subprocess, "run", fail)
    with pytest.raises(subprocess.CalledProcessError):
        module.test_installed_wheel(
            metadata_wheel(tmp_path / "candidate.whl"),
            "3.13",
            facade_wheel(tmp_path),
            reasoning_wheel(tmp_path),
            tmp_path / "yaml-inputs.json",
        )
    assert len(commands) == 1


@pytest.mark.parametrize("version, upper", [("3.11", "3.12"), ("3.13", "3.14"), ("3.15", "3.16")])
def test_isolated_project_is_bounded_to_its_native_wheel_abi(tmp_path, monkeypatch, version, upper):
    module = driver(monkeypatch)
    commands = []
    monkeypatch.setattr(module, "verify_metadata", lambda wheel: None)
    monkeypatch.setattr(module, "verify_licenses", lambda wheel: None)
    monkeypatch.setattr(
        module.subprocess, "run", lambda command, **kwargs: commands.append(command)
    )
    module.test_installed_wheel(
        tmp_path / "candidate.whl",
        version,
        facade_wheel(tmp_path),
        reasoning_wheel(tmp_path),
        tmp_path / "yaml-inputs.json",
    )
    initialize = next(command for command in commands if command[:2] == ["uv", "init"])
    assert initialize[initialize.index("--python") + 1] == f">={version},<{upper}"
