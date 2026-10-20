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


def test_source_archive_extraction_invalidates_stale_native_builds(tmp_path):
    import io
    import shutil
    import tarfile

    if sys.platform == "win32":
        pytest.skip("The manylinux source-extraction path runs on Unix hosts")
    assert shutil.which("cargo") and shutil.which("tar")
    project = tmp_path / "source-build"
    (project / "src").mkdir(parents=True)
    manifest = '[package]\nname = "archive-freshness"\nversion = "0.1.0"\nedition = "2024"\n'
    old_source = 'fn main() { println!("old"); }\n'
    new_source = 'fn main() { println!("new"); }\n'
    (project / "Cargo.toml").write_text(manifest)
    (project / "src/main.rs").write_text(old_source)
    target = tmp_path / "target"
    environment = os.environ.copy()
    environment.update(CARGO_TARGET_DIR=str(target), CARGO_BUILD_BUILD_DIR=str(target / "build"))

    def build_and_read():
        subprocess.run(
            ["cargo", "build", "--offline", "--quiet"],
            cwd=project,
            env=environment,
            check=True,
            capture_output=True,
        )
        return subprocess.check_output([target / "debug/archive-freshness"], text=True).strip()

    assert build_and_read() == "old"
    archives = tmp_path / "source-dist"
    archives.mkdir()
    archive = archives / "fixture.tar.gz"
    with tarfile.open(archive, "w:gz") as output:
        for name, body in (("Cargo.toml", manifest), ("src/main.rs", new_source)):
            content = body.encode()
            entry = tarfile.TarInfo("fixture/" + name)
            entry.size = len(content)
            entry.mtime = 1
            output.addfile(entry, io.BytesIO(content))
    original_bytes = archive.read_bytes()
    # Historical archive timestamps reproduce Cargo accepting the cached old binary.
    subprocess.run(
        ["tar", "-xzf", archive, "--strip-components=1", "-C", project],
        cwd=tmp_path,
        check=True,
        capture_output=True,
    )
    assert (project / "src/main.rs").read_text() == new_source
    assert build_and_read() == "old"

    extraction = next(
        line
        for line in (ROOT / ".github/workflows/ci.yml").read_text().splitlines()
        if line.strip().startswith("tar ")
    )
    subprocess.run(["sh", "-c", extraction], cwd=tmp_path, check=True, capture_output=True)
    assert build_and_read() == "new"
    assert archive.read_bytes() == original_bytes


def test_wheel_job_budget_covers_five_interpreters_and_verification():
    import re

    workflow = (ROOT / ".github/workflows/ci.yml").read_text()
    wheels = workflow.split("\n  wheels:\n", 1)[1].split("\n  native-coverage:\n", 1)[0]
    timeout = re.search(r"^    timeout-minutes: (\d+)$", wheels, re.MULTILINE)
    assert timeout is not None
    assert int(timeout.group(1)) == 60, "Cold native builds plus five wheels require a bounded hour"
    versions = " ".join(f"{abi[2]}.{abi.split('-')[0][3:]}" for abi in ABIS)
    assert f"for python in {versions}; do" in wheels
    assert " ".join(ABIS) in wheels
    assert "python scripts/test_wheel.py" in wheels
