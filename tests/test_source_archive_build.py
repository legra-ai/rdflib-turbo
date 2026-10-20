"""Source archives must rebuild changed native sources after a Cargo cache restore."""

import os
import subprocess
import sys
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


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
