"""Installed candidate metadata owns its independently pinned reference engine."""

import importlib.util
from pathlib import Path
from zipfile import ZipFile

import pytest


def collector():
    path = Path(__file__).resolve().parents[1] / "scripts/native_coverage.py"
    spec = importlib.util.spec_from_file_location("reference_collector", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def facade(tmp_path, requirements):
    path = tmp_path / "facade.whl"
    metadata = "Name: pyshacl-turbo\nVersion: 0.7.5\n"
    metadata += "".join(f"Requires-Dist: {value}\n" for value in requirements)
    with ZipFile(path, "w") as archive:
        archive.writestr("pyshacl_turbo-0.7.5.dist-info/METADATA", metadata)
    return path


@pytest.mark.parametrize("version", ["0.40.1", "0.40.2"])
def test_reference_pin_comes_from_original_candidate_metadata(tmp_path, version):
    requirement = f"pyshacl[oxigraph]=={version}"
    wheel = facade(tmp_path, [requirement + "; extra == 'test'"])
    assert collector().facade_reference_requirement(wheel) == requirement


@pytest.mark.parametrize(
    "requirements",
    [
        [],
        ["pyshacl>=0.40; extra == 'test'"],
        ["pyshacl==0.40.*; extra == 'test'"],
        ["pyshacl==0.40.2; extra == 'dev'"],
        ["pyshacl==0.40.1; extra == 'test'", "pyshacl==0.40.2; extra == 'test'"],
    ],
)
def test_missing_unpinned_or_ambiguous_reference_fails_before_install(tmp_path, requirements):
    with pytest.raises(ValueError, match=r"exact.*reference"):
        collector().facade_reference_requirement(facade(tmp_path, requirements))
