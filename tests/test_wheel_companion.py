"""Wheel environments select the same immutable companion as their native engine."""

import importlib.util
import subprocess
import sys
import tomllib
from pathlib import Path
from zipfile import ZipFile

import pytest

ROOT = Path(__file__).resolve().parents[1]
REVISION = "a" * 40
URL = "https://github.com/legra-ai/pyshacl-turbo"


def driver():
    spec = importlib.util.spec_from_file_location(
        "companion_wheel_driver", ROOT / "scripts/test_wheel.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def manifests(directory, source=None, engine=None):
    source = source or f'{{git="{URL}",rev="{REVISION}"}}'
    engine = engine or f'{{git="{URL}",rev="{REVISION}"}}'
    (directory / "pyproject.toml").write_text(f"[tool.uv.sources]\npyshacl-turbo={source}\n")
    (directory / "Cargo.toml").write_text(f"[dependencies]\npyshacl-turbo-engine={engine}\n")


def test_matching_immutable_companion_source(tmp_path, monkeypatch):
    manifests(tmp_path)
    module = driver()
    monkeypatch.setattr(module, "ROOT", tmp_path)
    assert module.companion_source() == (URL, REVISION)


@pytest.mark.parametrize(
    "source,engine",
    [
        ('{git="https://example.invalid/repo",rev="' + REVISION + '"}', None),
        ('{git="' + URL + '",rev="' + "b" * 40 + '"}', None),
        ('{git="' + URL + '",rev="main"}', None),
        ('{git="' + URL + '",branch="main"}', None),
        ('[{git="' + URL + '",rev="' + REVISION + '"}]', None),
        ('{git="' + URL + '",rev="' + REVISION + '",tag="v1"}', None),
        ('{path="../companion"}', None),
        ('{git=1,rev="' + REVISION + '"}', None),
        (None, '"0.5"'),
    ],
)
def test_missing_ambiguous_or_mismatched_sources_fail_fast(tmp_path, monkeypatch, source, engine):
    manifests(tmp_path, source, engine)
    module = driver()
    monkeypatch.setattr(module, "ROOT", tmp_path)
    with pytest.raises(ValueError, match="companion"):
        module.companion_source()


@pytest.mark.parametrize("missing", ("pyproject.toml", "Cargo.toml"))
def test_missing_source_declaration_is_rejected(tmp_path, monkeypatch, missing):
    manifests(tmp_path)
    (tmp_path / missing).write_text("")
    module = driver()
    monkeypatch.setattr(module, "ROOT", tmp_path)
    with pytest.raises(ValueError, match="companion"):
        module.companion_source()


def wheel(path, requirement="rdflib>=7.3,<8"):
    with ZipFile(path, "w") as archive:
        archive.writestr(
            "rdflib_turbo-0.0.0.dist-info/METADATA",
            "Metadata-Version: 2.3\nName: rdflib-turbo\nVersion: 0.0.0\n"
            f"Requires-Dist: {requirement}\n",
        )
        archive.writestr(
            "rdflib_turbo-0.0.0.dist-info/WHEEL", "Wheel-Version: 1.0\nTag: py3-none-any\n"
        )
        archive.writestr("rdflib_turbo-0.0.0.dist-info/RECORD", "")


@pytest.mark.parametrize("git", ("git+https://example.invalid/repo", "git+file:///repo"))
def test_git_requires_dist_is_rejected(tmp_path, git):
    archive = tmp_path / "runtime.whl"
    wheel(archive, "pyshacl-turbo @ " + git)
    with pytest.raises(ValueError, match="Git"):
        driver().verify_metadata(archive)


def test_registry_wheel_metadata_is_accepted(tmp_path):
    archive = tmp_path / "runtime.whl"
    wheel(archive)
    driver().verify_metadata(archive)


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


def test_isolated_project_receives_real_uv_git_source(tmp_path, monkeypatch):
    companion = tmp_path / "companion"
    companion.mkdir()
    (companion / "pyproject.toml").write_text(
        '[project]\nname="pyshacl-turbo"\nversion="0.5.0"\nrequires-python=">=3.11"\n'
    )
    for command in (
        ["git", "init", str(companion)],
        ["git", "-C", str(companion), "add", "pyproject.toml"],
        [
            "git",
            "-C",
            str(companion),
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "-m",
            "test: source fixture",
        ],
    ):
        subprocess.run(command, check=True, capture_output=True)
    revision = subprocess.check_output(
        ["git", "-C", str(companion), "rev-parse", "HEAD"], text=True
    ).strip()
    url = companion.as_uri()
    declaration = f'{{git="{url}",rev="{revision}"}}'
    manifests(tmp_path, declaration, declaration)
    archive = tmp_path / "rdflib_turbo-0.0.0-py3-none-any.whl"
    wheel(archive)
    module = driver()
    monkeypatch.setattr(module, "ROOT", tmp_path)
    monkeypatch.setattr(module, "verify_licenses", lambda path: None)
    original = subprocess.run
    commands = []
    snapshots = []

    def run(command, **kwargs):
        commands.append(command)
        if command[1] in {"sync", "run"}:
            project = Path(command[command.index("--project") + 1])
            snapshots.append(tomllib.loads((project / "pyproject.toml").read_text()))
            return subprocess.CompletedProcess(command, 0)
        return original(command, **kwargs)

    monkeypatch.setattr(module.subprocess, "run", run)
    python = f"{sys.version_info.major}.{sys.version_info.minor}"
    module.test_installed_wheel(archive, python)
    assert snapshots[0]["tool"]["uv"]["sources"]["pyshacl-turbo"] == {
        "git": url,
        "rev": revision,
    }
    assert "pyshacl-turbo" in snapshots[0]["project"]["dependencies"]
    assert not any("git+" in item for item in snapshots[0]["project"]["dependencies"])
    assert any(command[1:3] == ["python", "pin"] and command[-1] == python for command in commands)
    assert any("--verify-loaded" in command and "-I" in command for command in commands)
    assert any("--cov-fail-under=100" in command for command in commands)
