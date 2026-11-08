"""Build the stable-ABI wheel once in one container and propagate failures."""

import os
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
ABI = "cp311-cp311"
pytestmark = pytest.mark.skipif(sys.platform == "win32", reason="POSIX manylinux drivers")


def fake_command(directory, name, body):
    path = directory / name
    path.write_text("#!/bin/sh\n" + body)
    path.chmod(0o755)


def test_manylinux_builds_the_minimum_stable_abi_once(tmp_path):
    log = tmp_path / "calls"
    fake_command(tmp_path, "uv", 'printf "%s\\n" "$*" >> "$TURBO_TEST_LOG" || exit 1\n')
    result = subprocess.run(
        ["sh", str(ROOT / "scripts/build_manylinux_inner.sh"), "x86_64-unknown-linux-gnu"],
        env={
            **os.environ,
            "PATH": str(tmp_path) + os.pathsep + os.environ["PATH"],
            "TURBO_TEST_LOG": str(log),
        },
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
    calls = log.read_text().splitlines()
    assert len(calls) == 1
    assert f"--python /opt/python/{ABI}/bin/python" in calls[0]
    assert "--target x86_64-unknown-linux-gnu" in calls[0]
    assert "--compatibility manylinux_2_28" in calls[0]


def test_manylinux_build_failure_is_a_hard_error(tmp_path):
    log = tmp_path / "calls"
    fake_command(
        tmp_path,
        "uv",
        'printf "%s\\n" "$*" >> "$TURBO_TEST_LOG" || exit 1\nexit 7\n',
    )
    result = subprocess.run(
        ["sh", str(ROOT / "scripts/build_manylinux_inner.sh"), "x86_64-unknown-linux-gnu"],
        env={
            **os.environ,
            "PATH": str(tmp_path) + os.pathsep + os.environ["PATH"],
            "TURBO_TEST_LOG": str(log),
        },
        capture_output=True,
        text=True,
    )
    assert result.returncode != 0
    calls = log.read_text().splitlines()
    assert len(calls) == 1
    assert "cp311-cp311" in calls[0]


def test_manylinux_outer_enters_one_container_for_the_stable_abi(tmp_path):
    log = tmp_path / "calls"
    fake_command(tmp_path, "docker", 'printf "%s\\n" "$*" >> "$TURBO_TEST_LOG" || exit 1\n')
    source = tmp_path / "source-build"
    source.mkdir()
    (source / "pyproject.toml").touch()
    (source / "Cargo.lock").touch()
    result = subprocess.run(
        [
            "sh",
            str(ROOT / "scripts/build_manylinux.sh"),
            "example:image",
            "x86_64-unknown-linux-gnu",
        ],
        cwd=tmp_path,
        env={
            **os.environ,
            "PATH": str(tmp_path) + os.pathsep + os.environ["PATH"],
            "TURBO_TEST_LOG": str(log),
        },
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stderr
    calls = log.read_text().splitlines()
    assert len(calls) == 2
    assert calls[0].startswith("build ")
    assert calls[1].startswith("run --rm ")
    assert calls[1].endswith("sh /build-wheel.sh x86_64-unknown-linux-gnu")


def test_wheel_job_budget_covers_its_interpreters_and_verification():
    import re

    workflow = (ROOT / ".github/workflows/ci.yml").read_text()
    wheels = workflow.split("\n  wheels:\n", 1)[1].split("\n  native-coverage:\n", 1)[0]
    timeout = re.search(
        r"^    timeout-minutes: \$\{\{ matrix\.os == 'macos-15-intel' "
        r"&& (?P<intel>\d+) \|\| (?P<default>\d+) \}\}$",
        wheels,
        re.MULTILINE,
    )
    assert timeout is not None
    # One cold build plus five complete installed suites and runner setup.
    assert 35 + 9 * 5 + 3 <= int(timeout.group("default")) <= 90
    assert 35 + 12 * 5 + 3 <= int(timeout.group("intel")) <= 120
    assert "uv build --wheel --python 3.11" in wheels
    assert "for python" not in wheels
    assert "python scripts/test_wheel.py" in wheels


def test_clang_install_uses_only_the_images_base_repositories():
    dockerfile = (ROOT / "scripts/manylinux.Dockerfile").read_text()
    install = next(line for line in dockerfile.splitlines() if "clang-libs" in line)
    assert "--disablerepo='epel*'" in install
    assert "--nogpgcheck" not in dockerfile
    assert "sslverify=0" not in dockerfile
