"""Machine identity admits stdout only while retaining raw child diagnostics."""

import importlib.util
import os
import subprocess
import sys
from pathlib import Path

import pytest


@pytest.fixture
def helper():
    path = Path(__file__).resolve().parents[1] / "scripts/yaml_test_inputs.py"
    spec = importlib.util.spec_from_file_location("yaml_machine_test_inputs", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def command(stdout, stderr, code=0):
    return [
        sys.executable,
        "-I",
        "-B",
        "-c",
        f"import sys;sys.stdout.write({stdout!r});sys.stdout.flush();"
        f"sys.stderr.write({stderr!r});sys.stderr.flush();sys.exit({code})",
    ]


@pytest.mark.parametrize("identity", ["3.13.16\n", "uv 0.13.5 (fixture)\n"])
def test_machine_identity_keeps_progress_out_of_stdout(helper, tmp_path, capfd, identity):
    actual = helper._run(
        command(identity, "Downloading interpreter\n"),
        tmp_path,
        dict(os.environ),
        machine_identity=True,
    )
    assert actual == identity
    assert capfd.readouterr().err == "Downloading interpreter" + os.linesep


def test_expected_version_on_stderr_cannot_replace_wrong_stdout(helper, tmp_path, capfd):
    actual = helper._run(
        command("3.13.15\n", "3.13.16\n"), tmp_path, dict(os.environ), machine_identity=True
    )
    assert actual.strip() != helper.PYTHON
    assert capfd.readouterr().err == "3.13.16" + os.linesep


def test_machine_child_failure_keeps_status_stdout_and_raw_stderr(helper, tmp_path, capfd):
    with pytest.raises(subprocess.CalledProcessError) as caught:
        helper._run(
            command("3.13.16\n", "download failed\n", 7),
            tmp_path,
            dict(os.environ),
            machine_identity=True,
        )
    assert caught.value.returncode == 7
    assert caught.value.output == "3.13.16\n"
    assert capfd.readouterr().err == "download failed" + os.linesep


def test_build_output_still_merges_both_streams(helper, tmp_path, capfd):
    actual = helper._run(command("build output\n", "build warning\n"), tmp_path, dict(os.environ))
    assert actual == "build output\nbuild warning\n"
    assert capfd.readouterr().err == ""
