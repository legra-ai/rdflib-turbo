"""Candidate construction has an isolated namespace and exact source inputs."""

import importlib.util
from pathlib import Path

import pytest


def helper():
    spec = importlib.util.spec_from_file_location(
        "build_facade_candidate",
        Path(__file__).resolve().parents[1] / "scripts/build_facade_candidate.py",
    )
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


@pytest.mark.parametrize("name", ["release-packages", "dist", "originals"])
def test_candidate_output_cannot_use_a_release_namespace(tmp_path, name):
    with pytest.raises(ValueError, match="candidate-facade namespace"):
        helper().require_output(tmp_path / name, [])
    assert not (tmp_path / name).exists()


def test_candidate_output_must_be_outside_frozen_sources(tmp_path):
    source = tmp_path / "source"
    with pytest.raises(ValueError, match="outside frozen source"):
        helper().require_output(source / "candidate-facade-inputs", [source])


def test_source_binding_cannot_escape_the_adjacent_provider_role():
    manifest = {
        "tool": {"uv": {"sources": {"rdflib-turbo": {"path": "../../outside", "editable": True}}}}
    }
    with pytest.raises(ValueError, match="adjacent package peer"):
        helper().peer_name(manifest, "rdflib-turbo")


def test_artifact_references_survive_cross_job_relocation(tmp_path):
    root = tmp_path / "candidate-facade-inputs"
    archive = root / "candidate-artifacts/source.tar.gz"
    assert helper().artifact_reference(root, archive) == "candidate-artifacts/source.tar.gz"
    with pytest.raises(ValueError, match="outside candidate artifact"):
        helper().artifact_reference(root, tmp_path / "outside.whl")
