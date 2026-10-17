"""Release inventory must agree with the targets used to build the wheels."""

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
        version = targets[triple]["macos-deployment-target"].replace(".", "_")
        platforms.append(f"macosx_{version}_{architecture}")
    return ["rdflib_turbo-0.2.0a1.tar.gz"] + [
        f"rdflib_turbo-0.2.0a1-cp311-abi3-{platform}.whl" for platform in platforms
    ]


def test_inventory_accepts_the_configured_platform_builds(tmp_path):
    for name in archive_names():
        (tmp_path / name).touch()
    assert release_packages.inventory(tmp_path, "0.2.0a1") == sorted(tmp_path.iterdir())


def test_inventory_rejects_an_unexpected_platform(tmp_path):
    for name in archive_names():
        (tmp_path / name.replace("win_amd64", "win_arm64")).touch()
    with pytest.raises(ValueError, match="Missing platform wheel: win_amd64"):
        release_packages.inventory(tmp_path, "0.2.0a1")
