"""Packaging validation must not resolve or install runtime/source peers."""

import os
import shutil
import subprocess
import tomllib
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]


def test_packaging_uses_an_independent_locked_project():
    tools = ROOT / "tools/packaging"
    manifest = tomllib.loads((tools / "pyproject.toml").read_text())
    assert manifest["project"]["dependencies"] == []
    assert manifest["dependency-groups"]["packaging"] == ["twine==7.0.0"]
    assert not manifest.get("tool", {}).get("uv", {}).get("sources")
    lock = tomllib.loads((tools / "uv.lock").read_text())
    names = {package["name"] for package in lock["package"]}
    assert "twine" in names
    assert not names & {"rdflib", "rdflib-turbo", "pyshacl-turbo", "owlrl-turbo", "maturin"}
    workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
    checks = [
        step["run"]
        for job in workflow["jobs"].values()
        for step in job.get("steps", [])
        if "twine check" in step.get("run", "")
    ]
    assert checks
    assert all(
        "uv --project tools/packaging run --locked --only-group packaging twine check --strict"
        in command
        for command in checks
    )


def test_packaging_sync_and_command_work_without_any_source_peer(tmp_path):
    project = tmp_path / "tools/packaging"
    shutil.copytree(ROOT / "tools/packaging", project, ignore=shutil.ignore_patterns(".venv"))
    environment = {
        name: value
        for name, value in os.environ.items()
        if name
        not in {"VIRTUAL_ENV", "PYTHONPATH", "PYTHONHOME", "UV_PROJECT", "UV_PROJECT_ENVIRONMENT"}
    }
    environment["UV_PYTHON"] = "3.13.16"
    subprocess.run(
        [
            "uv",
            "--project",
            str(project),
            "sync",
            "--locked",
            "--only-group",
            "packaging",
            "--no-build",
        ],
        env=environment,
        capture_output=True,
        text=True,
        check=True,
    )
    result = subprocess.run(
        [
            "uv",
            "--project",
            str(project),
            "run",
            "--locked",
            "--no-sync",
            "--only-group",
            "packaging",
            "twine",
            "--version",
        ],
        env=environment,
        capture_output=True,
        text=True,
        check=True,
    )
    assert "twine version 7.0.0" in result.stdout
