"""PR preparation validates authored commits, not GitHub's synthetic merge title."""

import runpy
import subprocess
from pathlib import Path

import pytest


@pytest.mark.parametrize("header", ["docs(benchmarks): add companion suite", "invalid subject"])
def test_preparation_handles_synthetic_pr_merge(tmp_path, monkeypatch, header):
    root = Path(__file__).resolve().parents[1]
    monkeypatch.chdir(tmp_path)
    monkeypatch.syspath_prepend(str(root / "scripts"))
    (tmp_path / "pyproject.toml").write_text('[project]\nversion = "0.1.0a2"\n')
    output = tmp_path / "output"
    monkeypatch.setenv("GITHUB_OUTPUT", str(output))

    def git(arguments, **kwargs):
        if arguments[1] == "rev-parse":
            return "a" * 40 + "\n"
        assert arguments[:2] == ["git", "log"]
        subjects = [header]
        if "--no-merges" not in arguments:
            subjects.insert(0, "Merge 123456 into abcdef")
        return "\n".join(subjects) + "\n"

    monkeypatch.setattr(subprocess, "check_output", git)
    if header == "invalid subject":
        with pytest.raises(ValueError, match="invalid subject"):
            runpy.run_path(str(root / "scripts/prepare_release.py"))
        assert not output.exists()
    else:
        runpy.run_path(str(root / "scripts/prepare_release.py"))
        assert output.read_text() == "version=0.1.0a2\nrevision=" + "a" * 40 + "\n"
