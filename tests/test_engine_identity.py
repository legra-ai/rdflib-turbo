"""The runtime must report the canonical engine revision it actually declares."""

import tomllib
from pathlib import Path

import pyshacl_turbo
from rdflib_turbo import _native


def test_engine_revision_matches_the_declared_source():
    root = Path(__file__).resolve().parents[1]
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    revision = manifest["dependencies"]["pyshacl-turbo-engine"]["rev"]
    assert _native.SHACL_ENGINE_REVISION == revision


def test_actual_compiled_shared_engine_matches_the_companion():
    assert _native.SHACL_ENGINE_FINGERPRINT == pyshacl_turbo._native.SHACL_ENGINE_FINGERPRINT
