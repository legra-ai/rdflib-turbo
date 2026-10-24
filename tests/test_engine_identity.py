"""The runtime exposes its owned source and the facade uses its actual classes."""

import tomllib
from pathlib import Path

import pyshacl_turbo
from rdflib_turbo import _native


def test_engine_source_matches_the_declared_provider_path():
    root = Path(__file__).resolve().parents[1]
    manifest = tomllib.loads((root / "Cargo.toml").read_text())
    source = manifest["dependencies"]["pyshacl-turbo-engine"]["path"]
    assert _native.SHACL_ENGINE_SOURCE == source == "crates/engine"


def test_companion_uses_the_actual_provider_compiler_class():
    assert _native.shacl.CompiledShapes is pyshacl_turbo._native.CompiledShapes
