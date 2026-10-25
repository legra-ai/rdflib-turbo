"""Every source test must collect against installed packages in isolated import mode."""

import subprocess
import sys
from pathlib import Path


def test_complete_suite_collects_without_adding_source_packages_to_python_path(tmp_path):
    tests = Path(__file__).resolve().parent
    result = subprocess.run(
        [
            sys.executable,
            "-I",
            "-B",
            "-m",
            "pytest",
            str(tests),
            "--collect-only",
            "--import-mode=importlib",
            "-q",
            "-p",
            "no:cacheprovider",
        ],
        cwd=tmp_path,
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stdout[-12000:] + result.stderr
    assert "tests collected" in result.stdout
