"""Build every interpreter in one container and stop at the first failed build."""

import os
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
ABIS = ("cp311-cp311", "cp312-cp312", "cp313-cp313", "cp314-cp314", "cp315-cp315")
pytestmark = pytest.mark.skipif(sys.platform == "win32", reason="POSIX manylinux drivers")


def fake_command(directory, name, body):
    path = directory / name
    path.write_text("#!/bin/sh\n" + body)
    path.chmod(0o755)


def test_manylinux_builds_all_interpreters_in_the_same_container(tmp_path):
    log = tmp_path / "calls"
    fake_command(tmp_path, "uv", 'printf "%s\\n" "$*" >> "$TURBO_TEST_LOG" || exit 1\n')
    result = subprocess.run(
        ["sh", str(ROOT / "scripts/build_manylinux_inner.sh"), "x86_64-unknown-linux-gnu", *ABIS],
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
    assert len(calls) == 5
    for call, abi in zip(calls, ABIS, strict=True):
        assert f"--python /opt/python/{abi}/bin/python" in call
        assert "--target x86_64-unknown-linux-gnu" in call
        assert "--compatibility manylinux_2_28" in call


def test_manylinux_failed_interpreter_stops_before_later_builds(tmp_path):
    log = tmp_path / "calls"
    fake_command(
        tmp_path,
        "uv",
        'printf "%s\\n" "$*" >> "$TURBO_TEST_LOG" || exit 1\n'
        'case "$*" in *cp312-cp312*) exit 7 ;; esac\n',
    )
    result = subprocess.run(
        ["sh", str(ROOT / "scripts/build_manylinux_inner.sh"), "x86_64-unknown-linux-gnu", *ABIS],
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
    assert len(calls) == 2
    assert "cp311-cp311" in calls[0] and "cp312-cp312" in calls[1]


def test_manylinux_outer_enters_one_container_for_all_five_abis(tmp_path):
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
            *ABIS,
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
    assert "sh /build-wheel.sh x86_64-unknown-linux-gnu " + " ".join(ABIS) in calls[1]
