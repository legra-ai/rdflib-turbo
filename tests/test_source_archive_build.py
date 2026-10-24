"""Source archives must rebuild changed native sources after a Cargo cache restore."""

import io
import os
import shutil
import subprocess
import sys
import tarfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_source_archive_extraction_invalidates_stale_native_builds(tmp_path):
    assert shutil.which("cargo")
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
    executable = target / "debug/archive-freshness"
    if os.name == "nt":
        executable = executable.with_suffix(".exe")

    def build_and_read():
        subprocess.run(
            ["cargo", "build", "--offline", "--quiet"],
            cwd=project,
            env=environment,
            check=True,
            capture_output=True,
        )
        return subprocess.check_output([executable], text=True).strip()

    assert build_and_read() == "old"
    archives = tmp_path / "source-dist"
    archives.mkdir()
    archive = archives / "fixture.tar.gz"
    with tarfile.open(archive, "w:gz") as output:
        for name, body in (
            ("Cargo.toml", manifest),
            ("Cargo.lock", (project / "Cargo.lock").read_text()),
            ("src/main.rs", new_source),
        ):
            content = body.encode()
            entry = tarfile.TarInfo("source-build/" + name)
            entry.size = len(content)
            entry.mtime = 1
            output.addfile(entry, io.BytesIO(content))
    original_bytes = archive.read_bytes()
    # Historical archive timestamps reproduce Cargo accepting the cached old binary.
    with tarfile.open(archive, "r:gz") as package:
        package.extractall(tmp_path, filter="data")
    assert (project / "src/main.rs").read_text() == new_source
    assert build_and_read() == "old"

    cached_binary = executable.read_bytes()
    # Only the fixture's source is removed; its actual stale Cargo outputs and
    # stable source path remain the same for the fresh extraction/build.
    shutil.rmtree(project)
    started = int(time.time())
    subprocess.run(
        [
            sys.executable,
            "-I",
            "-B",
            ROOT / "scripts/extract_source.py",
            "--archive-directory",
            archives,
            "--destination",
            project,
        ],
        cwd=tmp_path,
        check=True,
        capture_output=True,
    )
    assert executable.read_bytes() == cached_binary
    assert (project / "src/main.rs").stat().st_mtime >= started
    assert build_and_read() == "new"
    assert archive.read_bytes() == original_bytes
