"""Native coverage must include Python execution and every owned source file."""

import hashlib
import importlib.util
import json
from pathlib import Path

import pytest


def harness():
    path = Path(__file__).resolve().parents[1] / "scripts/native_coverage.py"
    spec = importlib.util.spec_from_file_location("native_coverage", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_repository_unmapped_source_inventory_matches_current_sources():
    root = Path(__file__).resolve().parents[1]
    inventory = json.loads((root / "native-coverage-sources.json").read_text())
    for name, entry in inventory.items():
        assert hashlib.sha256((root / name).read_bytes()).hexdigest() == entry["sha256"]


def file_report(path, covered, count=10):
    return {
        "filename": str(path),
        "summary": {
            name: {"count": count, "covered": covered} for name in ("lines", "regions", "functions")
        },
    }


def test_owned_inventory_cannot_disappear_from_the_denominator(tmp_path):
    module = harness()
    src = tmp_path / "src"
    src.mkdir()
    (src / "lib.rs").write_text("fn main() {}")
    (src / "uncovered.rs").write_text("fn uncovered() {}")
    report = {"data": [{"files": [file_report(src / "lib.rs", 10)]}]}
    with pytest.raises(RuntimeError, match=r"uncovered\.rs"):
        module.summarize(report, tmp_path)
    report["data"][0]["files"].append(file_report(src / "uncovered.rs", 0))
    summary = module.summarize(report, tmp_path)
    assert summary["owned"]["lines"] == {"count": 20, "covered": 10, "percent": 50.0}
    assert summary["branches"] is None


def test_dependencies_do_not_inflate_owned_coverage(tmp_path):
    module = harness()
    src = tmp_path / "src"
    src.mkdir()
    (src / "lib.rs").write_text("fn main() {}")
    report = {
        "data": [
            {
                "files": [
                    file_report(src / "lib.rs", 2),
                    file_report(tmp_path / "vendor/library/src/lib.rs", 10),
                    file_report(tmp_path / "tests/integration.rs", 10),
                    file_report(tmp_path.parent / "registry/dependency.rs", 10),
                ]
            }
        ]
    }
    summary = module.summarize(report, tmp_path)
    assert summary["owned"]["lines"]["percent"] == 20.0
    assert summary["vendored"]["lines"]["percent"] == 100.0


def test_environment_is_parsed_without_executing_shell_text():
    module = harness()
    assert module.parse_environment("RUSTFLAGS='-C instrument-coverage'\nCARGO_LLVM_COV=1\n") == {
        "RUSTFLAGS": "-C instrument-coverage",
        "CARGO_LLVM_COV": "1",
    }
    with pytest.raises(ValueError):
        module.parse_environment("RUSTFLAGS=x; touch /tmp/should-never-execute")


def test_rust_only_profiles_cannot_pass_as_python_integration_coverage():
    module = harness()
    with pytest.raises(RuntimeError, match="Python"):
        module.require_python_execution(
            {"files": {"src/lib.rs": {"lines": {"covered": 0}}}}, "src/lib.rs"
        )
    module.require_python_execution(
        {"files": {"src/lib.rs": {"lines": {"covered": 2}}}}, "src/lib.rs"
    )


def test_cargo_private_environment_assignments_are_supported():
    assert (
        harness().parse_environment(
            "RUSTFLAGS='-C instrument-coverage'\n__CARGO_LLVM_COV_RUSTC_WRAPPER=1\n"
        )["__CARGO_LLVM_COV_RUSTC_WRAPPER"]
        == "1"
    )


def test_pinned_cargo_wrapper_carries_instrumentation_flags():
    environment = harness().parse_environment(
        "__CARGO_LLVM_COV_RUSTC_WRAPPER=1\n"
        "__CARGO_LLVM_COV_RUSTC_WRAPPER_RUSTFLAGS='-C\x1finstrument-coverage'\n"
        "RUSTC_WRAPPER=/tools/cargo-llvm-cov\n"
    )
    assert environment["RUSTC_WRAPPER"] == "/tools/cargo-llvm-cov"


def test_zero_mapping_declarations_require_an_exact_source_hash(tmp_path):
    module = harness()
    (tmp_path / "src").mkdir()
    source = tmp_path / "src/types.rs"
    source.write_text("pub type Id = u64;")
    inventory = {
        "src/types.rs": {
            "sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "reason": "Declarations only; no executable LLVM mapping",
        }
    }
    (tmp_path / "native-coverage-sources.json").write_text(json.dumps(inventory))
    summary = module.summarize({"data": [{"files": []}]}, tmp_path)
    assert summary["unmapped_non_executable"] == inventory
    source.write_text("pub fn now_executable() {}")
    with pytest.raises(RuntimeError, match="source hash"):
        module.summarize({"data": [{"files": []}]}, tmp_path)


def test_declaration_inventory_cannot_hide_an_executable_mapping_with_the_same_hash(tmp_path):
    module = harness()
    (tmp_path / "src").mkdir()
    source = tmp_path / "src/types.rs"
    source.write_text("pub fn typed_error_message() {}")
    inventory = {
        "src/types.rs": {
            "sha256": hashlib.sha256(source.read_bytes()).hexdigest(),
            "reason": "Former declaration-only inventory",
        }
    }
    (tmp_path / "native-coverage-sources.json").write_text(json.dumps(inventory))
    with pytest.raises(RuntimeError, match="now has executable coverage"):
        module.summarize({"data": [{"files": [file_report(source, covered=1)]}]}, tmp_path)


def test_profile_mapping_cannot_shrink_the_owned_denominator():
    module = harness()
    expected = {"files": {"src/lib.rs": {key: {"count": 10} for key in module.METRICS}}}
    actual = {"files": {"src/lib.rs": {key: {"count": 9} for key in module.METRICS}}}
    with pytest.raises(RuntimeError, match="denominator"):
        module.verify_denominators(actual, expected)
    module.verify_denominators(expected, expected)


def test_embedded_python_uses_the_selected_interpreters_standard_library(monkeypatch, tmp_path):
    module = harness()
    calls = []
    prefix = tmp_path / "Python runtime"
    interpreter = tmp_path / "venv" / "python"

    def run(arguments, environment, *, capture=False):
        calls.append((arguments, environment, capture))
        return f"{prefix}\n"

    monkeypatch.setattr(module, "run", run)
    original = {"PATH": "/bin", "LLVM_PROFILE_FILE": "profiles/%p.profraw"}
    configured = module.embedded_python_environment(interpreter, original)
    assert configured["PYTHONHOME"] == str(prefix)
    assert "PYTHONHOME" not in original
    assert configured["LLVM_PROFILE_FILE"] == original["LLVM_PROFILE_FILE"]
    assert calls[0][0][0] == interpreter
    assert calls[0][2] is True


def test_embedded_python_rejects_an_invalid_runtime_prefix(monkeypatch):
    module = harness()
    monkeypatch.setattr(module, "run", lambda *args, **kwargs: "relative-prefix\n")
    with pytest.raises(RuntimeError, match="absolute runtime prefix"):
        module.embedded_python_environment(Path("/venv/bin/python"), {})


@pytest.mark.parametrize("previous", [None, "/existing/lib"])
def test_embedded_python_linux_loader_uses_the_selected_interpreters_library(
    monkeypatch, tmp_path, previous
):
    module = harness()
    monkeypatch.setattr(module.sys, "platform", "linux")
    prefix = str(tmp_path / "python")
    library = str(tmp_path / "python/lib")
    monkeypatch.setattr(
        module,
        "run",
        lambda args, env, capture=False: library if "sysconfig" in args[-1] else prefix,
    )
    original = {} if previous is None else {"LD_LIBRARY_PATH": previous}
    configured = module.embedded_python_environment(str(tmp_path / "python/bin/python"), original)
    assert configured["LD_LIBRARY_PATH"] == library + (
        module.os.pathsep + previous if previous else ""
    )
    assert original == ({} if previous is None else {"LD_LIBRARY_PATH": previous})


def test_embedded_python_rejects_an_invalid_library_directory(monkeypatch, tmp_path):
    module = harness()
    monkeypatch.setattr(module.sys, "platform", "linux")
    monkeypatch.setattr(
        module,
        "run",
        lambda args, env, capture=False: (
            "relative" if "sysconfig" in args[-1] else str(tmp_path / "python")
        ),
    )
    with pytest.raises(RuntimeError, match="absolute library directory"):
        module.embedded_python_environment("/selected/python/bin/python", {})


def test_native_uv_project_pins_the_requested_interpreter(tmp_path):
    import json
    import os
    import subprocess
    import sys

    module = harness()
    environment = dict(os.environ)
    for name in ("VIRTUAL_ENV", "UV_PYTHON", "UV_PROJECT_ENVIRONMENT", "UV_PROJECT"):
        environment.pop(name, None)
    project = tmp_path / "isolated-project"
    module.initialize_project(project, sys.executable, environment)
    output = subprocess.check_output(
        [
            "uv",
            "run",
            "--project",
            str(project),
            "python",
            "-c",
            "import sys,json;print(json.dumps(list(sys.version_info[:2])))",
        ],
        env=environment,
        text=True,
    )
    assert json.loads(output) == list(sys.version_info[:2])
    assert (project / ".python-version").is_file()
