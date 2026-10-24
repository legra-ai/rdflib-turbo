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
    # Intel run 39636861664 built all five wheels in ~58 minutes and hit the
    # hour limit before verification. Budget a cold build, four ABI rebuilds,
    # installed tests for every wheel, and runner setup without dropping stages.
    required_minutes = 35 + 7 * (len(ABIS) - 1) + 2 * len(ABIS) + 2
    assert required_minutes <= int(timeout.group("default")) <= 90
    # Run39985683383 exceeded120minutes:35cold+15perABI+8perinstalledsuite.
    # Partition only Intel into at mostthree interpreters per bounded job.
    matrix = wheels.split("        include:\n", 1)[1].split("    steps:\n", 1)[0]
    intel_versions = []
    for entry in matrix.split("          - ")[1:]:
        if entry.startswith("os: macos-15-intel\n"):
            match = re.search(r"python-versions: '([^']+)'", entry)
            assert match is not None
            intel_versions.append(match.group(1).split())
    assert sorted(map(len, intel_versions)) == [2, 3]
    for versions in intel_versions:
        required = 35 + 16 * (len(versions) - 1) + 9 * len(versions) + 3
        assert required <= int(timeout.group("intel")) <= 120
    assert "for python in ${{ matrix.python-versions }}; do" in wheels
    assert " ".join(ABIS) in wheels
    assert "python scripts/test_wheel.py" in wheels


def test_clang_install_uses_only_the_images_base_repositories():
    dockerfile = (ROOT / "scripts/manylinux.Dockerfile").read_text()
    install = next(line for line in dockerfile.splitlines() if "clang-libs" in line)
    assert "--disablerepo='epel*'" in install
    assert "--nogpgcheck" not in dockerfile
    assert "sslverify=0" not in dockerfile
