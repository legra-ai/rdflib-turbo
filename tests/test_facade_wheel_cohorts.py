"""Interpreter cohorts retain exact pure facade acceptance on every wheel."""

import importlib.util
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
VERSIONS = ("3.11", "3.12", "3.13", "3.14", "3.15")


def driver(monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / "scripts"))
    spec = importlib.util.spec_from_file_location(
        "facade_cohort_driver", ROOT / "scripts/test_wheel.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def wheels(directory, versions):
    paths = {}
    for version in versions:
        tag = "cp" + version.replace(".", "")
        path = directory / f"rdflib_turbo-0.2.0a1-{tag}-{tag}-macosx_11_0_x86_64.whl"
        path.touch()
        paths[version] = path
    return paths


@pytest.mark.parametrize("versions", (VERSIONS[:3], VERSIONS[3:]))
def test_explicit_cohort_keeps_the_exact_facade_for_every_interpreter(
    tmp_path, monkeypatch, versions
):
    module = driver(monkeypatch)
    expected = wheels(tmp_path, versions)
    selected = module.select_wheels(tmp_path, "0.2.0a1", versions=versions)
    facade = tmp_path / "explicit-facade.whl"
    called = []
    monkeypatch.setattr(
        module,
        "test_installed_wheel",
        lambda path, version, peer: called.append((path, version, peer)),
    )
    module.test_matrix(selected, facade, versions=versions)
    assert called == [(expected[version], version, facade) for version in versions]


@pytest.mark.parametrize("versions", ((), ("3.10",), ("3.11", "3.11"), ("3.15", "3.11")))
def test_invalid_cohort_fails_before_testing(tmp_path, monkeypatch, versions):
    with pytest.raises(ValueError):
        driver(monkeypatch).select_wheels(tmp_path, "0.2.0a1", versions=versions)


def test_missing_extra_and_wrong_matrix_inventory_fail(tmp_path, monkeypatch):
    module = driver(monkeypatch)
    paths = wheels(tmp_path, VERSIONS[:2])
    with pytest.raises(ValueError):
        module.select_wheels(tmp_path, "0.2.0a1", versions=VERSIONS[:3])
    with pytest.raises(ValueError):
        module.select_wheels(tmp_path, "0.2.0a1", versions=VERSIONS[:1])
    with pytest.raises(ValueError):
        module.test_matrix(paths, tmp_path / "facade.whl", versions=VERSIONS[:1])


def test_main_accepts_both_required_facade_and_selected_cohort(tmp_path, monkeypatch):
    module = driver(monkeypatch)
    (tmp_path / "dist").mkdir()
    expected = wheels(tmp_path / "dist", VERSIONS[3:])
    (tmp_path / "pyproject.toml").write_text('[project]\nversion = "0.2.0a1"\n')
    facade = tmp_path / "explicit-facade.whl"
    monkeypatch.setattr(module, "ROOT", tmp_path)
    monkeypatch.setattr(module, "require_facade_wheel", lambda path: path)
    monkeypatch.setattr(
        sys,
        "argv",
        ["test_wheel.py", "--shacl-wheel", str(facade), "--python-versions", *VERSIONS[3:]],
    )
    called = []
    monkeypatch.setattr(
        module,
        "test_installed_wheel",
        lambda path, version, peer: called.append((path, version, peer)),
    )
    module.main()
    assert called == [(expected[version], version, facade) for version in VERSIONS[3:]]
    monkeypatch.setattr(sys, "argv", ["test_wheel.py", "--shacl-wheel", str(facade)])
    with pytest.raises(ValueError):
        module.main()
