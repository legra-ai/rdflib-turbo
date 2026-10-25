"""The compiled matcher identity includes its actual source and feature policy."""

import shutil
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def test_actual_matcher_identity_changes_for_vendor_only_mutation(tmp_path):
    original = ROOT / "vendor/fancy-regex-0.20.0"
    fork = tmp_path / "vendor/fancy-regex-0.20.0"
    shutil.copytree(
        original,
        fork,
        ignore=shutil.ignore_patterns("target", ".git"),
        copy_function=shutil.copyfile,
    )
    for path in original.rglob("*"):
        relative = path.relative_to(original)
        if path.is_file() and not {"target", ".git"}.intersection(relative.parts):
            assert (fork / relative).read_bytes() == path.read_bytes()
    shutil.copyfile(
        ROOT / "vendor/turbo-source-identity.rs", fork.parent / "turbo-source-identity.rs"
    )
    assert (fork.parent / "turbo-source-identity.rs").read_bytes() == (
        ROOT / "vendor/turbo-source-identity.rs"
    ).read_bytes()
    project = tmp_path / "probe"
    (project / "src").mkdir(parents=True)
    (project / "Cargo.toml").write_text(
        '[package]\nname="matcher-identity-probe"\nversion="0.0.0"\nedition="2024"\n'
        '[workspace]\n[dependencies]\nfancy-regex={path="../vendor/fancy-regex-0.20.0",'
        'default-features=false,features=["std","perf","unicode","python-re"]}\n'
    )
    (project / "src/main.rs").write_text(
        'fn main() { println!("{}", fancy_regex::implementation_fingerprint()); }\n'
    )
    subprocess.run(
        ["cargo", "generate-lockfile", "--offline"],
        cwd=project,
        capture_output=True,
        text=True,
        check=True,
    )

    def identity():
        result = subprocess.run(
            ["cargo", "run", "--locked", "--offline", "--quiet", "--jobs", "2"],
            cwd=project,
            capture_output=True,
            text=True,
            check=True,
        )
        fingerprint = result.stdout.strip()
        assert len(fingerprint) == 64
        int(fingerprint, 16)
        return fingerprint

    baseline = identity()
    source = fork / "src/vm.rs"
    original_source = source.read_bytes()
    source.write_bytes(original_source + b"\n// Actual vendor-only identity regression.\n")
    assert identity() != baseline
    source.write_bytes(original_source)
    assert identity() == baseline
    manifest = fork / "Cargo.toml"
    manifest.write_bytes(manifest.read_bytes() + b"\n# Manifest identity regression.\n")
    assert identity() != baseline
