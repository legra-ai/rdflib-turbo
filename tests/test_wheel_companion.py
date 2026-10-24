"""Provider wheel environments require the exact pure companion artifact."""

import importlib.util
import subprocess
import sys
import tomllib
import types
from pathlib import Path
from zipfile import ZipFile

import pytest

ROOT = Path(__file__).resolve().parents[1]


def driver():
    spec = importlib.util.spec_from_file_location(
        "companion_wheel_driver", ROOT / "scripts/test_wheel.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def wheel(path, requirement="rdflib>=7.3,<8", *, name="rdflib-turbo", extra=None, pure=True):
    package = name.replace("-", "_")
    members = {
        f"{package}-0.0.0.dist-info/METADATA": (
            f"Metadata-Version: 2.3\nName: {name}\nVersion: 0.0.0\nRequires-Dist: {requirement}\n"
        ).encode(),
        f"{package}-0.0.0.dist-info/WHEEL": (
            f"Wheel-Version: 1.0\nRoot-Is-Purelib: {str(pure).lower()}\nTag: py3-none-any\n"
        ).encode(),
        f"{package}-0.0.0.dist-info/RECORD": b"",
    }
    if name == "pyshacl-turbo":
        members.update(
            {
                "pyshacl_turbo/__init__.py": b"facade\n",
                "pyshacl_turbo/_native.py": b"shared bindings\n",
                "pyshacl_turbo/assets/probe.txt": b"asset\n",
            }
        )
    if name == "owlrl-turbo":
        members["owlrl_turbo/__init__.py"] = b"reasoning companion\n"
    members.update(extra or {})
    with ZipFile(path, "w") as archive:
        for member, payload in members.items():
            archive.writestr(member, payload)
    return path


def companion(tmp_path, **kwargs):
    return wheel(tmp_path / "pyshacl_turbo-0.0.0-py3-none-any.whl", name="pyshacl-turbo", **kwargs)


def reasoning(tmp_path, **kwargs):
    return wheel(tmp_path / "owlrl_turbo-0.0.0-py3-none-any.whl", name="owlrl-turbo", **kwargs)


def test_reasoning_original_wheel_is_required_before_any_provider_collection(tmp_path):
    path = reasoning(tmp_path)
    assert driver().require_owlrl_wheel(path) == path.resolve()
    for missing in (None, tmp_path / "missing.whl", tmp_path):
        with pytest.raises(ValueError):
            driver().require_owlrl_wheel(missing)


def test_reasoning_cannot_supply_a_second_native_runtime(tmp_path):
    with pytest.raises(ValueError):
        driver().require_owlrl_wheel(
            reasoning(tmp_path, extra={"owlrl_turbo/engine.so": b"native"})
        )


def test_explicit_pure_facade_artifact_is_required(tmp_path):
    path = companion(tmp_path)
    assert driver().require_facade_wheel(path) == path.resolve()
    for missing in (None, tmp_path / "missing.whl", tmp_path):
        with pytest.raises(ValueError, match="facade wheel"):
            driver().require_facade_wheel(missing)


@pytest.mark.parametrize("native", ["engine.so", "engine.so.1", "engine.pyd", "engine.dll"])
def test_companion_artifact_cannot_include_another_native_runtime(tmp_path, native):
    path = companion(tmp_path, extra={"pyshacl_turbo/" + native: b"native"})
    with pytest.raises(RuntimeError, match="native"):
        driver().require_facade_wheel(path)


def test_companion_artifact_requires_universal_metadata(tmp_path):
    with pytest.raises(RuntimeError, match="pure"):
        driver().require_facade_wheel(companion(tmp_path, pure=False))


def test_companion_distribution_name_must_identify_the_facade(tmp_path):
    path = wheel(
        tmp_path / "other.whl",
        name="other",
        extra={"pyshacl_turbo/__init__.py": b"", "pyshacl_turbo/_native.py": b""},
    )
    with pytest.raises(ValueError, match="pyshacl-turbo"):
        driver().require_facade_wheel(path)


def test_companion_cannot_introduce_a_native_git_requirement(tmp_path):
    path = companion(tmp_path, requirement="rdflib-turbo @ git+https://example.invalid/native")
    with pytest.raises(ValueError, match="Git"):
        driver().require_facade_wheel(path)


@pytest.mark.parametrize("git", ("git+https://example.invalid/repo", "git+file:///repo"))
def test_git_requires_dist_is_rejected(tmp_path, git):
    archive = wheel(tmp_path / "runtime.whl", "pyshacl-turbo @ " + git)
    with pytest.raises(ValueError, match="Git"):
        driver().verify_metadata(archive)


def test_registry_wheel_metadata_is_accepted(tmp_path):
    driver().verify_metadata(wheel(tmp_path / "runtime.whl"))


def test_missing_or_ambiguous_wheel_metadata_is_rejected(tmp_path):
    archive = tmp_path / "runtime.whl"
    with ZipFile(archive, "w"):
        pass
    with pytest.raises(ValueError, match="METADATA"):
        driver().verify_metadata(archive)
    wheel(archive)
    with ZipFile(archive, "a") as output:
        output.writestr("other.dist-info/METADATA", "Name: other\n")
    with pytest.raises(ValueError, match="METADATA"):
        driver().verify_metadata(archive)


def installed_facade(tmp_path, monkeypatch):
    path = companion(tmp_path)
    environment = tmp_path / "environment"
    with ZipFile(path) as archive:
        for name in archive.namelist():
            if name.startswith("pyshacl_turbo/"):
                destination = environment / "site-packages" / name
                destination.parent.mkdir(parents=True, exist_ok=True)
                destination.write_bytes(archive.read(name))
    package = types.ModuleType("pyshacl_turbo")
    package.__file__ = str(environment / "site-packages/pyshacl_turbo/__init__.py")
    facade = types.ModuleType("pyshacl_turbo._native")
    facade.CompiledShapes, facade.TargetData = object(), object()
    provider = types.ModuleType("rdflib_turbo._native")
    provider.shacl = types.SimpleNamespace(
        CompiledShapes=facade.CompiledShapes,
        TargetData=facade.TargetData,
        SHACL_ENGINE_FINGERPRINT="compiled-source-observation",
    )
    for name, module in (
        ("pyshacl_turbo", package),
        ("pyshacl_turbo._native", facade),
        ("rdflib_turbo._native", provider),
    ):
        monkeypatch.setitem(sys.modules, name, module)
    return path, environment, package, facade


def test_loaded_facade_verifier_checks_actual_bytes_and_both_shared_classes(tmp_path, monkeypatch):
    path, environment, _, _ = installed_facade(tmp_path, monkeypatch)
    measured = driver().facade_checker().verify_loaded_facade(path, environment)
    assert measured["engine_fingerprint"] == "compiled-source-observation"
    assert "pyshacl_turbo/assets/probe.txt" in measured["payload"]


@pytest.mark.parametrize("kind", ["CompiledShapes", "TargetData"])
def test_loaded_facade_rejects_a_distinct_provider_class(tmp_path, monkeypatch, kind):
    path, environment, _, facade = installed_facade(tmp_path, monkeypatch)
    setattr(facade, kind, object())
    with pytest.raises(RuntimeError, match="another native provider"):
        driver().facade_checker().verify_loaded_facade(path, environment)


@pytest.mark.parametrize("damage", ["changed", "extra", "outside"])
def test_loaded_facade_rejects_changed_extra_or_source_payloads(tmp_path, monkeypatch, damage):
    path, environment, package, _ = installed_facade(tmp_path, monkeypatch)
    directory = Path(package.__file__).parent
    if damage == "changed":
        (directory / "assets/probe.txt").write_text("changed")
    elif damage == "extra":
        (directory / "extra.dll").write_text("native")
    else:
        environment = tmp_path / "other-environment"
    with pytest.raises(RuntimeError, match=r"differs|outside"):
        driver().facade_checker().verify_loaded_facade(path, environment)


def test_isolated_project_receives_only_exact_wheel_sources(tmp_path, monkeypatch):
    facade = companion(tmp_path)
    owl = reasoning(tmp_path)
    (tmp_path / "pyproject.toml").write_text(
        '[project]\nname="source-fixture"\nversion="0.0.0"\nrequires-python=">=3.11"\n'
        '[dependency-groups]\ncoverage=["pyshacl-turbo==0.0.0"]\n'
        f'[tool.uv.sources]\npyshacl-turbo={{path="{facade.as_posix()}"}}\n'
    )
    subprocess.run(
        ["uv", "lock", "--project", str(tmp_path), "--python", sys.executable],
        check=True,
        capture_output=True,
    )
    archive = wheel(tmp_path / "rdflib_turbo-0.0.0-py3-none-any.whl")
    module = driver()
    monkeypatch.setattr(module, "ROOT", tmp_path)
    monkeypatch.setattr(module, "verify_licenses", lambda path: None)
    original, commands, snapshots = subprocess.run, [], []

    def run(command, **kwargs):
        commands.append(command)
        if command[1] in {"sync", "run"}:
            project = Path(command[command.index("--project") + 1])
            snapshots.append((project, tomllib.loads((project / "pyproject.toml").read_text())))
            return subprocess.CompletedProcess(command, 0)
        return original(command, **kwargs)

    monkeypatch.setattr(module.subprocess, "run", run)
    python = f"{sys.version_info.major}.{sys.version_info.minor}"
    module.test_installed_wheel(archive, python, facade, owl)
    project, manifest = snapshots[0]
    for name, expected in (
        ("pyshacl-turbo", facade),
        ("rdflib-turbo", archive),
        ("owlrl-turbo", owl),
    ):
        declaration = manifest["tool"]["uv"]["sources"][name]
        assert set(declaration) == {"path"}
        assert (project / declaration["path"]).resolve() == expected.resolve()
    assert not any("git+" in item for item in manifest["project"]["dependencies"])
    assert any(command[1:3] == ["python", "pin"] and command[-1] == python for command in commands)
    assert any("--verify-loaded" in command and str(facade) in command for command in commands)
    installations = [
        command for command in commands if command[1] == "add" and str(archive) in command
    ]
    assert len(installations) == 1
    assert all(str(path) in installations[0] for path in (archive, facade, owl))
    assert any("--verify-loaded" in command and str(owl) in command for command in commands)
    assert any("--cov-fail-under=100" in command for command in commands)
    assert not any(
        "--rev" in command or "--config-settings-package" in command for command in commands
    )
