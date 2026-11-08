"""One stable-ABI build per platform retains every supported installed suite."""

import hashlib
import importlib.util
import json
import subprocess
import sys
import tomllib
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]


def test_release_uses_the_python_311_stable_abi():
    manifest = tomllib.loads((ROOT / "pyproject.toml").read_text())
    assert "pyo3/abi3-py311" in manifest["tool"]["maturin"]["features"]
    assert manifest["project"]["requires-python"] == ">=3.11,<3.16"


def test_each_platform_builds_once_and_tests_all_five_interpreters():
    workflow = yaml.safe_load((ROOT / ".github/workflows/ci.yml").read_text())
    job = workflow["jobs"]["wheels"]
    assert job["strategy"]["matrix"] == "${{ fromJSON(needs.prepare.outputs.wheel-matrix) }}"
    spec = importlib.util.spec_from_file_location(
        "abi3_policy", ROOT / "scripts/release_wheel_policy.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    matrix = module.select(tomllib.loads((ROOT / "pyproject.toml").read_text()))["matrix"][
        "include"
    ]
    assert len(matrix) == 5
    assert len({entry["target"] for entry in matrix}) == 5
    assert all(entry["python-versions"] == "3.11 3.12 3.13 3.14 3.15" for entry in matrix)
    runs = [step.get("run", "") for step in job["steps"]]
    native = next(command for command in runs if "uv build --wheel" in command)
    assert "--python 3.11" in native
    assert "for python" not in native.split("else", 1)[0]
    linux = next(command for command in runs if "sh scripts/build_manylinux.sh" in command)
    assert "cp312" not in linux.split("else", 1)[0] and "cp315" not in linux.split("else", 1)[0]
    assert any("--python-versions ${{ matrix.python-versions }}" in command for command in runs)
    assert any("abi3audit --strict" in command for command in runs)


def test_generated_unicode_data_has_complete_portable_provenance():
    directory = ROOT / "crates/python/src/host_unicode_data"
    records = json.loads((directory / "provenance.json").read_text())
    assert [row["python"].split(".")[1] for row in records] == ["11", "12", "13", "14", "15"]
    expected = {"provenance.json"}
    for row in records:
        minor = row["python"].split(".")[1]
        assert row["verified_scalars"] == 0x110000
        for kind in ("flags", "lower"):
            name = f"{minor}-{kind}.bin"
            expected.add(name)
            data = (directory / name).read_bytes()
            assert len(data) == row[kind + "_bytes"]
            assert hashlib.sha256(data).hexdigest() == row[kind + "_sha256"]
    assert {path.name for path in directory.iterdir()} == expected


def test_unicode_tables_match_every_host_codepoint():
    result = subprocess.run(
        [sys.executable, "-I", "-B", str(ROOT / "scripts/generate_host_unicode.py"), "--check"],
        capture_output=True,
        text=True,
    )
    assert result.returncode == 0, result.stdout + result.stderr


def test_unicode_forwarding_does_not_create_an_unmappable_rust_source_file():
    # LLVM cannot map a source containing only `pub use`; keep the alias in
    # its existing executable owner instead of weakening the source denominator.
    assert not (ROOT / "src/inference/diagnostic_datatypes/unicode.rs").exists()
