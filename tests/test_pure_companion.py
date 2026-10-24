"""Every Python stage needs the exact pure OWL companion, including its assets."""

import hashlib
import importlib.util
import sys
import types
from pathlib import Path
from zipfile import ZipFile

import pytest

ROOT = Path(__file__).resolve().parents[1]


def helper():
    spec = importlib.util.spec_from_file_location(
        "pure_companion", ROOT / "scripts/pure_companion.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def fixture(tmp_path, *, native=False, version="0.1.0a3"):
    source = tmp_path / "source"
    package = source / "src/owlrl_turbo"
    (package / "assets").mkdir(parents=True)
    (source / "pyproject.toml").write_text('[project]\nname="owlrl-turbo"\nversion="0.1.0a3"\n')
    payload = {"owlrl_turbo/__init__.py": b"pure\n", "owlrl_turbo/assets/example.txt": b"asset\n"}
    for name, body in payload.items():
        (source / "src" / name).write_bytes(body)
    wheel = tmp_path / "owlrl_turbo-0.1.0a3-py3-none-any.whl"
    with ZipFile(wheel, "w") as output:
        for name, body in payload.items():
            output.writestr(name, body)
        output.writestr(
            "owlrl_turbo-0.1.0a3.dist-info/METADATA", f"Name: owlrl-turbo\nVersion: {version}\n"
        )
        output.writestr(
            "owlrl_turbo-0.1.0a3.dist-info/WHEEL",
            "Wheel-Version: 1.0\nRoot-Is-Purelib: true\nTag: py3-none-any\n",
        )
        if native:
            output.writestr("owlrl_turbo/duplicate.so", b"native")
    return source, wheel, payload


def test_original_pure_wheel_matches_all_source_package_files(tmp_path):
    source, wheel, payload = fixture(tmp_path)
    assert helper().verify_payload(source, wheel) == {
        name: hashlib.sha256(body).hexdigest() for name, body in payload.items()
    }


@pytest.mark.parametrize("damage", ("native", "source", "version"))
def test_native_or_different_source_and_metadata_are_hard_errors(tmp_path, damage):
    source, wheel, _ = fixture(
        tmp_path, native=damage == "native", version="wrong" if damage == "version" else "0.1.0a3"
    )
    if damage == "source":
        (source / "src/owlrl_turbo/assets/example.txt").write_text("changed")
    with pytest.raises(ValueError):
        helper().verify_payload(source, wheel)


def installed(tmp_path, monkeypatch):
    source, wheel, _ = fixture(tmp_path)
    venv = tmp_path / "venv"
    package = venv / "site-packages/owlrl_turbo"
    (package / "assets").mkdir(parents=True)
    for path in (source / "src/owlrl_turbo").rglob("*"):
        if path.is_file():
            (package / path.relative_to(source / "src/owlrl_turbo")).write_bytes(path.read_bytes())
    module = types.ModuleType("owlrl_turbo")
    module.__file__ = str(package / "__init__.py")
    monkeypatch.setitem(sys.modules, "owlrl_turbo", module)
    return wheel, venv, package


def test_installed_companion_is_proved_before_running_provider_tests(tmp_path, monkeypatch):
    wheel, venv, _ = installed(tmp_path, monkeypatch)
    proof = helper().verify_loaded(wheel, venv)
    assert proof["wheel_sha256"] == hashlib.sha256(wheel.read_bytes()).hexdigest()
    assert "owlrl_turbo/assets/example.txt" in proof["payload"]


@pytest.mark.parametrize("damage", ("extra", "changed", "origin"))
def test_source_import_or_installed_payload_drift_is_rejected(tmp_path, monkeypatch, damage):
    wheel, venv, package = installed(tmp_path, monkeypatch)
    if damage == "extra":
        (package / "extra.py").write_text("extra")
    elif damage == "changed":
        (package / "assets/example.txt").write_text("changed")
    else:
        venv = tmp_path / "other"
    with pytest.raises(ValueError):
        helper().verify_loaded(wheel, venv)
