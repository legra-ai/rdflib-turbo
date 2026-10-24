"""Python lint tools must run without building the native project."""

import os
import subprocess
import sys
import tomllib
from pathlib import Path


def test_commit_lint_runs_when_project_build_backend_is_unavailable(tmp_path):
    root = Path(__file__).resolve().parents[1]
    lock = tomllib.loads((root / "uv.lock").read_text())
    ruff = next(package["version"] for package in lock["package"] if package["name"] == "ruff")
    (tmp_path / "pyproject.toml").write_text(
        '[build-system]\nrequires = []\nbuild-backend = "blocked_backend"\n'
        'backend-path = ["."]\n\n[project]\nname = "lint-build-sentinel"\n'
        'version = "0.1.0"\nrequires-python = ">=3.11"\n\n'
        f'[project.optional-dependencies]\ntest = ["ruff=={ruff}"]\n\n'
        f'[dependency-groups]\ndev = ["ruff=={ruff}"]\n'
    )
    (tmp_path / "blocked_backend.py").write_text(
        'raise RuntimeError("Lint invoked the native project build backend")\n'
    )
    for directory in ("python", "tests", "scripts", "bin"):
        (tmp_path / directory).mkdir()
    (tmp_path / "python/probe.py").write_text("value = 1\n")
    cargo = tmp_path / "bin/cargo"
    cargo.write_text('#!/bin/sh\nprintf "%s\\n" "$*" >> cargo-invocations\n')
    cargo.chmod(0o755)
    formatter = tmp_path / "scripts/fmt.sh"
    formatter.write_text("#!/bin/sh\nexit 0\n")
    formatter.chmod(0o755)
    hook = tmp_path / "pre-commit"
    hook.write_bytes((root / ".githooks/pre-commit").read_bytes())
    environment = dict(os.environ)
    for name in ("VIRTUAL_ENV", "UV_PROJECT", "UV_PROJECT_ENVIRONMENT", "UV_NO_SYNC"):
        environment.pop(name, None)
    environment["PATH"] = str(tmp_path / "bin") + os.pathsep + environment["PATH"]
    environment["UV_PYTHON"] = sys.executable
    locked = subprocess.run(
        ["uv", "lock"],
        cwd=tmp_path,
        env=environment,
        check=False,
        capture_output=True,
        text=True,
    )
    assert locked.returncode == 0, locked.stdout + locked.stderr
    result = subprocess.run(
        ["sh", str(hook)],
        cwd=tmp_path,
        env=environment,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, result.stdout + result.stderr
    assert (tmp_path / "cargo-invocations").read_text().splitlines() == [
        "clippy --workspace --lib --tests --locked -- -D warnings"
    ]
