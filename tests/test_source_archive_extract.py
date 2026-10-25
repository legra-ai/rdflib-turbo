"""Source extraction treats drive prefixes as local paths and preserves bytes."""

import importlib.util
import io
import os
import stat
import subprocess
import sys
import tarfile
import time
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


def helper():
    spec = importlib.util.spec_from_file_location(
        "extract_source", ROOT / "scripts/extract_source.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def archive(path, entries):
    with tarfile.open(path, "w:gz") as output:
        for name, kind, content in entries:
            member = tarfile.TarInfo(name)
            member.type, member.mtime, member.mode = kind, 1, 0o755
            if kind == tarfile.REGTYPE:
                member.size = len(content)
                output.addfile(member, io.BytesIO(content))
            else:
                member.linkname = content.decode()
                output.addfile(member)
    return path


def test_absolute_drive_or_literal_colon_archive_has_fresh_byte_exact_output(tmp_path):
    source = tmp_path / "original.tar.gz"
    if not source.drive:
        # A colon is a legal literal filename on a POSIX test host; Windows
        # already supplies the actual drive prefix through its absolute path.
        source = source.with_name("D:original.tar.gz")
    assert ":" in str(source)
    payload = b"source\r\n\x00\xff"
    archive(
        source,
        [("package", tarfile.DIRTYPE, b""), ("package/src/body.rs", tarfile.REGTYPE, payload)],
    )
    original, original_mtime = source.read_bytes(), source.stat().st_mtime_ns
    started = int(time.time())
    destination = tmp_path / "source-build"
    helper().extract(source, destination)
    extracted = destination / "src/body.rs"
    assert extracted.read_bytes() == payload
    assert extracted.stat().st_mtime >= started
    assert source.read_bytes() == original
    assert source.stat().st_mtime_ns == original_mtime


def test_existing_destination_is_a_hard_error(tmp_path):
    source = archive(
        tmp_path / "original.tar.gz", [("package/body.rs", tarfile.REGTYPE, b"source")]
    )
    destination = tmp_path / "source-build"
    destination.mkdir()
    (destination / "kept").write_bytes(b"original")
    with pytest.raises(FileExistsError):
        helper().extract(source, destination)
    assert (destination / "kept").read_bytes() == b"original"


def test_executable_file_permissions_survive_without_special_permission_bits(tmp_path):
    source = tmp_path / "original.tar.gz"
    with tarfile.open(source, "w:gz") as output:
        script = tarfile.TarInfo("package/vendor/configure")
        script.size, script.mode, script.mtime = 2, 0o6755, 1
        output.addfile(script, io.BytesIO(b"#!"))
    destination = tmp_path / "source-build"
    helper().extract(source, destination)
    result = destination / "vendor/configure"
    assert result.read_bytes() == b"#!"
    assert not result.stat().st_mode & (stat.S_ISUID | stat.S_ISGID | stat.S_ISVTX)
    if os.name != "nt":
        assert stat.S_IMODE(result.stat().st_mode) == 0o755


@pytest.mark.parametrize(
    "name",
    [
        "/absolute",
        "../outside",
        "package/../outside",
        "package/C:drive",
        "package\\escape",
        "package//empty",
        "package/./dot",
        "bare-file",
    ],
)
def test_unsafe_members_are_rejected_before_creating_output(tmp_path, name):
    source = archive(tmp_path / "original.tar.gz", [(name, tarfile.REGTYPE, b"source")])
    destination = tmp_path / "source-build"
    with pytest.raises(ValueError):
        helper().extract(source, destination)
    assert not destination.exists()


@pytest.mark.parametrize(
    "entries",
    [
        [("package/file", tarfile.REGTYPE, b"first"), ("package/file", tarfile.REGTYPE, b"second")],
        [("package/file", tarfile.REGTYPE, b"first"), ("other/file", tarfile.REGTYPE, b"second")],
        [("package/link", tarfile.SYMTYPE, b"outside")],
        [("package/link", tarfile.LNKTYPE, b"package/file")],
        [("package/file", tarfile.FIFOTYPE, b"")],
        [
            ("package/file", tarfile.REGTYPE, b"parent"),
            ("package/file/child", tarfile.REGTYPE, b"child"),
        ],
    ],
)
def test_duplicate_roots_links_special_files_and_file_parents_are_invalid(tmp_path, entries):
    source = archive(tmp_path / "original.tar.gz", entries)
    destination = tmp_path / "source-build"
    with pytest.raises(ValueError):
        helper().extract(source, destination)
    assert not destination.exists()


def test_cli_selects_exactly_one_original_archive_without_shell_globs(tmp_path):
    directory = tmp_path / "source-dist"
    directory.mkdir()
    source = archive(
        directory / "original.tar.gz", [("package/body.rs", tarfile.REGTYPE, b"source")]
    )
    destination = tmp_path / "source-build"
    command = [
        sys.executable,
        "-I",
        "-B",
        str(ROOT / "scripts/extract_source.py"),
        "--archive-directory",
        str(directory),
        "--destination",
        str(destination),
    ]
    subprocess.run(command, check=True, capture_output=True)
    assert (destination / "body.rs").read_bytes() == b"source"
    source.rename(directory / "second.tar.gz")
    archive(directory / "extra.tar.gz", [("package/body.rs", tarfile.REGTYPE, b"extra")])
    failed = subprocess.run(
        [*command[:-1], str(tmp_path / "other")], check=False, capture_output=True, text=True
    )
    assert failed.returncode != 0
    assert "exactly one" in failed.stderr
    assert not (tmp_path / "other").exists()
