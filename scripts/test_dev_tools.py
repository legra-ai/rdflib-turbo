"""Lint tooling must resolve without installing the native runtime or siblings."""

import subprocess
import tempfile
import unittest
from pathlib import Path


class DevelopmentToolTests(unittest.TestCase):
    def test_ruff_isolated_from_runtime_dependencies(self):
        root = Path(__file__).resolve().parents[1]
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run(
                [
                    "uv",
                    "run",
                    "--project",
                    str(root / "tools/dev"),
                    "--locked",
                    "--only-group",
                    "dev",
                    "ruff",
                    "--version",
                ],
                cwd=directory,
                capture_output=True,
                text=True,
                check=False,
            )
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(result.stdout.strip(), "ruff 0.17.4")


if __name__ == "__main__":
    unittest.main()
