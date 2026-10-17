"""Install and test built wheels in isolated Python 3.11 and 3.14 environments."""

import subprocess
import tempfile
from pathlib import Path

root = Path(__file__).resolve().parents[1]
wheels = list((root / "dist").glob("*.whl"))
if len(wheels) != 1:
    raise ValueError("Expected exactly one built wheel")
for version in ("3.11", "3.14"):
    with tempfile.TemporaryDirectory() as directory:
        project = Path(directory) / "project"
        subprocess.run(
            [
                "uv",
                "init",
                "--bare",
                "--no-workspace",
                "--vcs",
                "none",
                "--python",
                version,
                str(project),
            ],
            check=True,
        )
        subprocess.run(["uv", "python", "pin", "--project", str(project), version], check=True)
        subprocess.run(
            [
                "uv",
                "add",
                "--project",
                str(project),
                "--extra",
                "test",
                "--no-sync",
                str(wheels[0]),
            ],
            check=True,
        )
        subprocess.run(["uv", "sync", "--project", str(project), "--locked"], check=True)
        subprocess.run(
            [
                "uv",
                "run",
                "--project",
                str(project),
                "--locked",
                "pytest",
                str(root / "tests"),
                "-q",
                "--import-mode=importlib",
                "--cov=rdflib_turbo",
                "--cov-branch",
                "--cov-fail-under=100",
            ],
            cwd=directory,
            check=True,
        )
