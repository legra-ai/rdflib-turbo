"""Published badges must use validated CI counts and the tested source revision."""

import importlib.util
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
