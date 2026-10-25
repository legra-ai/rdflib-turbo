"""Compiled dependency identity must detect source and semantic profile drift."""

import json
import shutil
import subprocess
import tomllib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
HELPER = ROOT / "vendor/turbo-source-identity.rs"


def project(directory: Path) -> Path:
    assert HELPER.is_file(), "shared compiled dependency identity helper is missing"
    directory.mkdir()
    shutil.copyfile(HELPER, directory / "turbo-source-identity.rs")
    crate = directory / "optimizer"
    (crate / "src").mkdir(parents=True)
    (crate / "Cargo.toml").write_text(
        '[package]\nname="oxigraph"\nversion="0.0.0"\nedition="2024"\n'
        "[features]\nexact-term-storage=[]\nrdf-12=[]\nfuture-semantic=[]\n"
        "rocksdb=[]\nrocksdb-debug=[]\nrocksdb-pkg-config=[]\n"
        '[build-dependencies]\nsha2="=0.11.1"\n'
    )
    (crate / "build.rs").write_text(
        '#[path="../turbo-source-identity.rs"] mod identity;\nfn main() { identity::build(); }\n'
    )
    (crate / "src/main.rs").write_text(
        'fn main() { println!("{}\\n{}", '
        'env!("TURBO_IMPLEMENTATION_FINGERPRINT"), '
        'env!("TURBO_BACKEND_CAPABILITY_FINGERPRINT")); }\n'
    )
    subprocess.run(
        ["cargo", "generate-lockfile", "--offline"],
        cwd=crate,
        check=True,
        capture_output=True,
        text=True,
    )
    return crate


def identity(crate: Path, features: str = "") -> tuple[str, str]:
    command = ["cargo", "run", "--locked", "--offline", "--quiet"]
    if features:
        command.extend(["--features", features])
    result = subprocess.run(command, cwd=crate, check=True, capture_output=True, text=True)
    semantic, backend = result.stdout.strip().splitlines()
    assert len(semantic) == len(backend) == 64
    return semantic, backend


def test_dependency_source_and_unknown_semantic_features_are_attested(tmp_path):
    crate = project(tmp_path / "first")
    original, backend = identity(crate)
    # bounded: this fixture contains only the handwritten tiny synthetic crate.
    copied = tmp_path / "copied"
    shutil.copytree(crate.parent, copied, ignore=shutil.ignore_patterns("target"))
    assert identity(copied / crate.name) == (original, backend)
    (crate / "src/optimizer.rs").write_text("pub fn optimize() -> u8 { 2 }\n")
    assert identity(crate)[0] != original
    (crate / "src/optimizer.rs").unlink()
    assert identity(crate)[0] == original
    for feature in ("exact-term-storage", "rdf-12", "future-semantic"):
        assert identity(crate, feature)[0] != original
    helper = crate.parent / "turbo-source-identity.rs"
    helper.write_text(helper.read_text() + "\n// identity helper tamper\n")
    assert identity(crate)[0] != original


def test_only_audited_persistence_capabilities_are_separate(tmp_path):
    crate = project(tmp_path / "backend")
    original, backend = identity(crate)
    for capability in ("rocksdb", "rocksdb-debug", "rocksdb-pkg-config"):
        semantic, changed_backend = identity(crate, capability)
        assert semantic == original
        assert changed_backend != backend


def test_actual_backend_feature_graph_does_not_leak_implicit_semantic_features(tmp_path):
    features = tomllib.loads((ROOT / "vendor/oxigraph-0.5.13/Cargo.toml").read_text())["features"]
    crate = tmp_path / "source"
    dependency = tmp_path / "backend"
    (crate / "src").mkdir(parents=True)
    (dependency / "src").mkdir(parents=True)
    (crate / "src/lib.rs").write_text("")
    (dependency / "src/lib.rs").write_text("")
    (dependency / "Cargo.toml").write_text(
        '[package]\nname="oxrocksdb-sys"\nversion="0.0.0"\n[features]\npkg-config=[]\n'
    )
    manifest = '[package]\nname="oxigraph"\nversion="0.0.0"\n[features]\n'
    capabilities = ("rocksdb", "rocksdb-debug", "rocksdb-pkg-config")
    for capability in capabilities:
        manifest += f"{capability}={json.dumps(features[capability])}\n"
    manifest += '[dependencies]\noxrocksdb-sys={path="../backend",optional=true}\n'
    (crate / "Cargo.toml").write_text(manifest)
    for capability in capabilities:
        result = subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--offline", "--features", capability],
            cwd=crate,
            check=True,
            capture_output=True,
            text=True,
        )
        metadata = json.loads(result.stdout)
        root = metadata["resolve"]["root"]
        active = next(
            node["features"] for node in metadata["resolve"]["nodes"] if node["id"] == root
        )
        assert set(active) <= set(capabilities), active
