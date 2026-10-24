"""Partition expensive interpreter jobs while preserving the exact global inventory."""

import importlib.util
import re
import sys
from collections import Counter
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]
VERSIONS = ("3.11", "3.12", "3.13", "3.14", "3.15")
TARGETS = {
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-apple-darwin",
    "aarch64-apple-darwin",
    "x86_64-pc-windows-msvc",
}


def wheel_job():
    workflow = (ROOT / ".github/workflows/ci.yml").read_text()
    return workflow.split("\n  wheels:\n", 1)[1].split("\n  native-coverage:\n", 1)[0]


def cohorts():
    matrix = wheel_job().split("        include:\n", 1)[1].split("    steps:\n", 1)[0]
    return [
        dict(re.findall(r"^\s*(?:- )?([a-z-]+): (.+)$", entry, re.MULTILINE))
        for entry in matrix.split("          - ")[1:]
    ]


def test_cohorts_cover_every_platform_interpreter_pair_exactly_once():
    rows = cohorts()
    actual = Counter(
        (row["target"], version)
        for row in rows
        for version in row["python-versions"].strip("'\"").split()
    )
    assert actual == Counter((target, version) for target in TARGETS for version in VERSIONS)
    split = {"x86_64-apple-darwin", "x86_64-pc-windows-msvc"}
    for target in split:
        partition = [row for row in rows if row["target"] == target]
        assert len(partition) == 2
        assert {row["python-versions"].strip("'\"") for row in partition} == {
            "3.11 3.12 3.13",
            "3.14 3.15",
        }
    assert len(rows) == 7
    for target in TARGETS - split:
        assert sum(row["target"] == target for row in rows) == 1
    artifacts = [row["artifact"] for row in rows]
    assert len(artifacts) == len(set(artifacts))
    assert all(artifact.startswith("release-wheel-") for artifact in artifacts)
    job = wheel_job()
    assert "uv python install ${{ matrix.python-versions }}" in job
    assert "for python in ${{ matrix.python-versions }}; do" in job
    assert "scripts/test_wheel.py --python-versions ${{ matrix.python-versions }}" in job
    assert "name: ${{ matrix.artifact }}" in job
    assert "timeout-minutes: ${{ matrix.os == 'macos-15-intel' && 120 || 90 }}" in job


def test_aggregate_requires_every_cohort_and_exact_complete_release_inventory():
    workflow = (ROOT / ".github/workflows/ci.yml").read_text()
    aggregate = workflow.split("\n  test:\n", 1)[1].split("\n  publish:\n", 1)[0]
    assert "needs: [prepare, verify, sdist, wheels, native-coverage]" in aggregate
    assert "if: always()" in aggregate
    assert "job['result'] != 'success'" in aggregate
    assert "pattern: release-*" in aggregate
    assert "python scripts/release_packages.py check-build" in aggregate
    assert workflow.count("\n  publish:\n") == 1


def driver(monkeypatch):
    monkeypatch.syspath_prepend(str(ROOT / "scripts"))
    spec = importlib.util.spec_from_file_location(
        "cohort_wheel_driver", ROOT / "scripts/test_wheel.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def wheels(directory, versions):
    paths = {}
    for version in versions:
        tag = "cp" + version.replace(".", "")
        path = directory / f"rdflib_turbo-0.2.0a1-{tag}-{tag}-macosx_11_0_x86_64.whl"
        path.touch()
        paths[version] = path
    return paths


@pytest.mark.parametrize("versions", (VERSIONS[:3], VERSIONS[3:]))
def test_explicit_cohort_selects_and_tests_every_matching_pair(tmp_path, monkeypatch, versions):
    module = driver(monkeypatch)
    expected = wheels(tmp_path, versions)
    selected = module.select_wheels(tmp_path, "0.2.0a1", versions=versions)
    assert selected == expected
    called = []
    monkeypatch.setattr(
        module, "test_installed_wheel", lambda path, version: called.append((path, version))
    )
    module.test_matrix(selected, versions=versions)
    assert called == [(expected[version], version) for version in versions]


@pytest.mark.parametrize("versions", ((), ("3.10",), ("3.11", "3.11"), ("3.15", "3.11")))
def test_invalid_cohort_fails_before_testing(tmp_path, monkeypatch, versions):
    with pytest.raises(ValueError):
        driver(monkeypatch).select_wheels(tmp_path, "0.2.0a1", versions=versions)


def test_missing_and_additional_cohort_archives_are_hard_errors(tmp_path, monkeypatch):
    module = driver(monkeypatch)
    paths = wheels(tmp_path, VERSIONS[:2])
    with pytest.raises(ValueError):
        module.select_wheels(tmp_path, "0.2.0a1", versions=VERSIONS[:3])
    paths["3.12"].unlink()
    wheels(tmp_path, VERSIONS[4:])
    with pytest.raises(ValueError):
        module.select_wheels(tmp_path, "0.2.0a1", versions=VERSIONS[:1])


def test_main_accepts_an_explicit_cohort_without_weakening_the_default(tmp_path, monkeypatch):
    module = driver(monkeypatch)
    (tmp_path / "dist").mkdir()
    expected = wheels(tmp_path / "dist", VERSIONS[3:])
    (tmp_path / "pyproject.toml").write_text('[project]\nversion = "0.2.0a1"\n')
    monkeypatch.setattr(module, "ROOT", tmp_path)
    monkeypatch.setattr(sys, "argv", ["test_wheel.py", "--python-versions", *VERSIONS[3:]])
    called = []
    monkeypatch.setattr(
        module, "test_installed_wheel", lambda path, version: called.append((path, version))
    )
    module.main()
    assert called == [(expected[version], version) for version in VERSIONS[3:]]
    monkeypatch.setattr(sys, "argv", ["test_wheel.py"])
    with pytest.raises(ValueError):
        module.main()
