"""The runtime must report the canonical engine revision it actually declares."""

import tomllib
from pathlib import Path

from rdflib_turbo import _native


def test_engine_revision_matches_the_declared_source():
    root = Path(__file__).resolve().parents[1]
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    revision = manifest["dependencies"]["pyshacl-turbo-engine"]["rev"]
    assert _native.SHACL_ENGINE_REVISION == revision
