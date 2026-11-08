"""Prepared source, rather than workflow age, owns immutable wheel inventories."""

import importlib.util
import tomllib
from pathlib import Path

import pytest
import yaml

ROOT = Path(__file__).resolve().parents[1]


def policy():
    spec = importlib.util.spec_from_file_location(
        "release_wheel_policy", ROOT / "scripts/release_wheel_policy.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_prepared_stable_abi_source_has_four_builds_and_twenty_installed_pairs():
    project = tomllib.loads((ROOT / "pyproject.toml").read_text())
    result = policy().select(project)
    assert result["stable-abi"] is True
    rows = result["matrix"]["include"]
    assert len(rows) == 4
    assert sum(len(row["python-versions"].split()) for row in rows) == 20
    assert all(row["os"] != "macos-15-intel" for row in rows)


def test_pending_original_cpython_release_retains_its_exact_seven_job_inventory():
    project = {
        "project": {"requires-python": ">=3.11"},
        "tool": {"maturin": {"features": ["pyo3/extension-module", "persistence"]}},
    }
    result = policy().select(project)
    assert result["stable-abi"] is False
    rows = result["matrix"]["include"]
    assert len(rows) == 7
    assert sum(len(row["python-versions"].split()) for row in rows) == 25
    assert len({row["artifact"] for row in rows}) == 7


@pytest.mark.parametrize("feature", ["pyo3/abi3", "pyo3/abi3-py312", "pyo3/abi3-py310"])
def test_unknown_stable_abi_policy_fails_instead_of_becoming_a_cpython_build(feature):
    project = tomllib.loads((ROOT / "pyproject.toml").read_text())
    project["tool"]["maturin"]["features"] = ["pyo3/extension-module", feature, "persistence"]
    with pytest.raises(ValueError):
        policy().select(project)


def test_controller_is_preserved_before_checkout_and_reads_the_prepared_source():
    workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
    prepare = workflow["jobs"]["prepare"]
    steps = prepare["steps"]
    copy = next(
        i
        for i, step in enumerate(steps)
        if "release-wheel-policy.py" in step.get("run", "") and "cp " in step["run"]
    )
    checkout = next(
        i
        for i, step in enumerate(steps)
        if step.get("name") == "Check out the prepared release revision"
    )
    select = next(i for i, step in enumerate(steps) if step.get("id") == "wheel-policy")
    assert copy < checkout < select
    assert '"$RUNNER_TEMP/release-wheel-policy.py"' in steps[select]["run"]
    assert (
        workflow["jobs"]["wheels"]["strategy"]["matrix"]
        == "${{ fromJSON(needs.prepare.outputs.wheel-matrix) }}"
    )
    audit = next(
        step
        for step in workflow["jobs"]["wheels"]["steps"]
        if "abi3audit --strict" in step.get("run", "")
    )
    assert audit["if"] == "needs.prepare.outputs.stable-abi == 'true'"
