"""Published badges must use validated CI counts and the tested source revision."""

import importlib.util
import json
import xml.etree.ElementTree as ET
from pathlib import Path

import pytest


def site():
    path = Path(__file__).resolve().parents[1] / "scripts/coverage_site.py"
    spec = importlib.util.spec_from_file_location("coverage_site", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def measurements():
    python = {
        "meta": {"branch_coverage": True},
        "totals": {
            "num_statements": 100,
            "covered_lines": 80,
            "num_branches": 10,
            "covered_branches": 5,
        },
    }
    rust = {
        "project": "rdflib-turbo",
        "revision": "runtime",
        "owlrl_revision": "facade",
        "date": "2026-10-16",
        "stages": {
            "combined": {
                "owned": {
                    key: {"count": 100, "covered": 70} for key in ("lines", "regions", "functions")
                }
            }
        },
    }
    return python, rust


def test_badges_keep_python_lines_branches_and_shared_rust_separate(tmp_path):
    python, rust = measurements()
    site().build_site(python, rust, tmp_path, "owlrl-turbo", "facade")
    assert (
        ET.parse(tmp_path / "python-lines.svg").getroot().attrib["aria-label"]
        == "Python lines: 80.0%"
    )
    assert (
        ET.parse(tmp_path / "python-branches.svg").getroot().attrib["aria-label"]
        == "Python branches: 50.0%"
    )
    assert (
        ET.parse(tmp_path / "rust-lines.svg").getroot().attrib["aria-label"]
        == "Shared Rust lines: 70.0%"
    )
    assert not (tmp_path / "rust-branches.svg").exists()
    assert "facade" in (tmp_path / "index.html").read_text()


def test_stale_or_unmeasured_numbers_cannot_be_published(tmp_path):
    python, rust = measurements()
    with pytest.raises(ValueError, match="revision"):
        site().build_site(python, rust, tmp_path, "owlrl-turbo", "wrong")
    python["meta"]["branch_coverage"] = False
    with pytest.raises(ValueError, match="branch"):
        site().build_site(python, rust, tmp_path, "rdflib-turbo", "runtime")


def test_badges_never_round_partial_coverage_up_to_100():
    root = ET.fromstring(site().render_badge("Rust lines", 9999, 10000))
    assert root.attrib["aria-label"] == "Rust lines: 99.9%"
    for covered, count in [(True, 10), (11, 10), (-1, 10), (1, 0), (float("nan"), 10)]:
        with pytest.raises(ValueError):
            site().render_badge("Rust lines", covered, count)


@pytest.mark.parametrize("project", ["pyshacl-turbo", "rdflib-turbo", "owlrl-turbo"])
def test_standard_shields_endpoints_publish_separate_measured_counts(tmp_path, project):
    python, rust = measurements()
    shared = project == "owlrl-turbo"
    rust["project"] = "rdflib-turbo" if shared else project
    revision = "facade" if shared else "runtime"
    site().build_site(python, rust, tmp_path, project, revision)
    expected = {
        "python-lines": ("Python lines", "80.0%", "a4a61d"),
        "python-branches": ("Python branches", "50.0%", "e05d44"),
        "coverage": ("Python coverage", "77.2%", "dfb317"),
        **{
            f"rust-{metric}": (
                ("Shared Rust " if shared else "Rust ") + metric,
                "70.0%",
                "dfb317",
            )
            for metric in ("lines", "regions", "functions")
        },
    }
    for name, (label, message, color) in expected.items():
        payload = json.loads((tmp_path / f"{name}.json").read_text())
        assert payload == {
            "schemaVersion": 1,
            "label": label,
            "message": message,
            "color": color,
        }
        assert ET.parse(tmp_path / f"{name}.svg").getroot().attrib["aria-label"] == (
            f"{label}: {message}"
        )


def test_standard_endpoint_never_rounds_partial_coverage_or_invents_branches(tmp_path):
    python, rust = measurements()
    python["totals"]["num_branches"] = 0
    python["totals"]["covered_branches"] = 0
    rust["stages"]["combined"]["owned"]["lines"] = {"count": 10000, "covered": 9999}
    site().build_site(python, rust, tmp_path, "owlrl-turbo", "facade")
    assert json.loads((tmp_path / "rust-lines.json").read_text())["message"] == "99.9%"
    assert json.loads((tmp_path / "python-branches.json").read_text())["message"] == "n/a"
