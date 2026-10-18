"""Release archives cover every tested CPython ABI and native platform exactly."""

import importlib.util
import tomllib
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "release_packages", ROOT / "scripts/release_packages.py"
)
release_packages = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release_packages)


def archive_names():
    project = tomllib.loads((ROOT / "pyproject.toml").read_text())
    targets = project["tool"]["maturin"]["target"]
    platforms = ["manylinux_2_28_x86_64", "manylinux_2_28_aarch64", "win_amd64"]
    for triple, architecture in [
        ("x86_64-apple-darwin", "x86_64"),
        ("aarch64-apple-darwin", "arm64"),
    ]:
        minimum = targets[triple]["macos-deployment-target"].replace(".", "_")
        platforms.append(f"macosx_{minimum}_{architecture}")
    return ["rdflib_turbo-0.2.0a1.tar.gz"] + [
        f"rdflib_turbo-0.2.0a1-cp{version}-cp{version}-{platform}.whl"
        for version in ("311", "312", "313", "314", "315")
        for platform in platforms
    ]


def populate(directory):
    for name in archive_names():
        (directory / name).touch()


def test_inventory_accepts_all_five_interpreters_and_five_platforms(tmp_path):
    populate(tmp_path)
    assert len(archive_names()) == 26
    assert release_packages.inventory(tmp_path, "0.2.0a1") == sorted(tmp_path.iterdir())


@pytest.mark.parametrize("version", ("311", "312", "313", "314", "315"))
def test_inventory_rejects_a_missing_interpreter_platform(tmp_path, version):
    populate(tmp_path)
    missing = f"rdflib_turbo-0.2.0a1-cp{version}-cp{version}-win_amd64.whl"
    (tmp_path / missing).unlink()
    with pytest.raises(ValueError, match=missing):
        release_packages.inventory(tmp_path, "0.2.0a1")


@pytest.mark.parametrize(
    "replacement",
    (
        "rdflib_turbo-0.2.0a1-cp311-abi3-win_amd64.whl",
        "rdflib_turbo-0.2.0a1-cp313-cp312-win_amd64.whl",
        "rdflib_turbo-0.2.0a1-cp313-cp313t-win_amd64.whl",
        "rdflib_turbo-0.2.0a1-cp313-cp313-win_arm64.whl",
        "rdflib_turbo-0.2.0a2-cp313-cp313-win_amd64.whl",
    ),
)
def test_inventory_rejects_wrong_abi_platform_or_version(tmp_path, replacement):
    populate(tmp_path)
    original = tmp_path / "rdflib_turbo-0.2.0a1-cp313-cp313-win_amd64.whl"
    original.rename(tmp_path / replacement)
    with pytest.raises(ValueError, match="win_amd64"):
        release_packages.inventory(tmp_path, "0.2.0a1")


@pytest.mark.parametrize(
    "extra", ("unexpected.txt", "rdflib_turbo-0.2.0a1-cp311-abi3-win_amd64.whl")
)
def test_inventory_rejects_extra_artifacts(tmp_path, extra):
    populate(tmp_path)
    (tmp_path / extra).touch()
    with pytest.raises(ValueError, match=extra):
        release_packages.inventory(tmp_path, "0.2.0a1")


def test_inventory_rejects_a_directory_with_an_archive_name(tmp_path):
    populate(tmp_path)
    path = tmp_path / "rdflib_turbo-0.2.0a1-cp314-cp314-win_amd64.whl"
    path.unlink()
    path.mkdir()
    with pytest.raises(ValueError, match="regular file"):
        release_packages.inventory(tmp_path, "0.2.0a1")
