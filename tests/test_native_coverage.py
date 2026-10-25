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


@pytest.mark.parametrize("same_sites", [False, True])
@pytest.mark.parametrize("runtime_platform", ["win32", "linux", "darwin"])
def test_embedded_python_binds_only_selected_runtime_sites(
    monkeypatch, tmp_path, same_sites, runtime_platform
):
    module = harness()
    monkeypatch.setattr(module.sys, "platform", runtime_platform)
    prefix = tmp_path / "selected-runtime"
    purelib = tmp_path / "selected venv" / "purelib"
    platlib = purelib if same_sites else tmp_path / "selected venv" / "platlib"
    calls = []

    def query(python, code, environment):
        arguments = [python, "-I", "-c", code]
        calls.append(arguments)
        if "get_path" in arguments[-1]:
            return json.dumps(
                {
                    "purelib": str(purelib),
                    "platlib": str(platlib),
                    "rdflib": str(purelib / "rdflib/__init__.py"),
                }
            )
        return str(prefix)

    monkeypatch.setattr(module, "runtime_query", query)
    original = {"PYTHONPATH": str(tmp_path / "untrusted-source"), "PYTHONNOUSERSITE": "0"}
    configured = module.embedded_python_environment("/selected/python", original)
    assert configured["PYTHONPATH"].split(module.os.pathsep) == list(
        dict.fromkeys([str(purelib), str(platlib)])
    )
    assert configured["PYTHONNOUSERSITE"] == "1"
    assert configured["PYTHONSAFEPATH"] == "1"
    assert all(arguments[1] == "-I" for arguments in calls)
    assert original == {"PYTHONPATH": str(tmp_path / "untrusted-source"), "PYTHONNOUSERSITE": "0"}


def test_embedded_python_imports_rdflib_from_actual_selected_uv_environment(tmp_path):
    import os
    import subprocess
    import sys

    module = harness()
    environment = dict(os.environ)
    for name in ("VIRTUAL_ENV", "UV_PYTHON", "UV_PROJECT_ENVIRONMENT", "UV_PROJECT"):
        environment.pop(name, None)
    project = tmp_path / "isolated-project"
    module.initialize_project(project, sys.executable, environment)
    subprocess.run(
        ["uv", "add", "--project", str(project), "--no-sync", "rdflib==7.6.0"],
        env=environment,
        check=True,
    )
    subprocess.run(
        ["uv", "sync", "--project", str(project), "--locked", "--no-build"],
        env=environment,
        check=True,
    )
    selected = (
        project / ".venv" / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
    )
    base = subprocess.check_output(
        [str(selected), "-I", "-c", "import sys; print(sys._base_executable)"],
        env=environment,
        text=True,
    ).strip()
    ambient = tmp_path / "ambient"
    (ambient / "rdflib").mkdir(parents=True)
    (ambient / "rdflib/__init__.py").write_text(
        "raise RuntimeError('ambient rdflib source imported')"
    )
    inherited = {**environment, "PYTHONPATH": str(ambient)}
    configured = module.embedded_python_environment(selected, inherited)
    # The base distribution cannot discover a UV venv itself, as with embedded CPython.
    actual = subprocess.check_output(
        [base, "-B", "-c", "import rdflib; print(rdflib.__file__)"],
        cwd=ambient,
        env=configured,
        text=True,
    ).strip()
    assert Path(actual).resolve().is_relative_to(project / ".venv")
    assert configured["PYTHONPATH"] != inherited["PYTHONPATH"]
    subprocess.run(
        ["uv", "remove", "--project", str(project), "--no-sync", "rdflib"],
        env=environment,
        check=True,
    )
    subprocess.run(
        ["uv", "sync", "--project", str(project), "--locked", "--no-build"],
        env=environment,
        check=True,
    )
    with pytest.raises(RuntimeError, match="rdflib"):
        module.embedded_python_environment(selected, inherited)


@pytest.mark.parametrize("runtime_platform", ["win32", "linux", "darwin"])
def test_embedded_python_uses_the_selected_interpreters_standard_library(
    monkeypatch, tmp_path, runtime_platform
):
    module = harness()
    monkeypatch.setattr(module.sys, "platform", runtime_platform)
    calls = []
    prefix = tmp_path / "Python runtime"
    interpreter = tmp_path / "venv" / "python"

    def query(python, code, environment):
        arguments = [python, "-I", "-c", code]
        calls.append((arguments, environment, True))
        if "get_path" in arguments[-1]:
            return json.dumps(
                {
                    "purelib": str(prefix),
                    "platlib": str(prefix),
                    "rdflib": str(prefix / "rdflib/__init__.py"),
                }
            )
        return f"{prefix}\n"

    monkeypatch.setattr(module, "runtime_query", query)
    original = {"PATH": "/bin", "LLVM_PROFILE_FILE": "profiles/%p.profraw"}
    configured = module.embedded_python_environment(interpreter, original)
    assert configured["PYTHONHOME"] == str(prefix)
    assert "PYTHONHOME" not in original
    assert configured["LLVM_PROFILE_FILE"] == original["LLVM_PROFILE_FILE"]
    assert calls[0][0][0] == interpreter
    assert calls[0][2] is True


@pytest.mark.parametrize("runtime_platform", ["win32", "linux", "darwin"])
def test_embedded_python_rejects_an_invalid_runtime_prefix(monkeypatch, runtime_platform):
    module = harness()
    monkeypatch.setattr(module.sys, "platform", runtime_platform)
    monkeypatch.setattr(module, "runtime_query", lambda *args, **kwargs: "relative-prefix\n")
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
        lambda args, env, capture=False: (
            json.dumps(
                {"purelib": prefix, "platlib": prefix, "rdflib": prefix + "/rdflib/__init__.py"}
            )
            if "get_path" in args[-1]
            else library
            if "sysconfig" in args[-1]
            else prefix
        ),
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
            json.dumps(
                {
                    "purelib": str(tmp_path),
                    "platlib": str(tmp_path),
                    "rdflib": str(tmp_path / "rdflib/__init__.py"),
                }
            )
            if "get_path" in args[-1]
            else "relative"
            if "sysconfig" in args[-1]
            else str(tmp_path / "python")
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


def test_windows_runtime_metadata_uses_create_process_without_posix_launcher(monkeypatch, tmp_path):
    from types import SimpleNamespace

    module = harness()
    prefix = str(tmp_path / "runtime")
    site = str(tmp_path / "selected/site-packages")
    calls = []

    def posix_forbidden(*args, **kwargs):
        raise AssertionError("Windows metadata query reached the POSIX collector launcher")

    def create_process(arguments, **kwargs):
        calls.append((arguments, kwargs))
        output = (
            json.dumps({"purelib": site, "platlib": site, "rdflib": site + "/rdflib/__init__.py"})
            if "sysconfig" in arguments[-1]
            else prefix
        )
        return SimpleNamespace(returncode=0, stdout=output + "\n")

    monkeypatch.setattr(module.sys, "platform", "win32")
    monkeypatch.setattr(module, "run", posix_forbidden)
    monkeypatch.setattr(module.subprocess, "run", create_process)
    result = module.embedded_python_environment(str(tmp_path / "python.exe"), {"SENTINEL": "kept"})
    assert result["PYTHONHOME"] == prefix
    assert result["PYTHONPATH"] == site
    assert len(calls) == 2
    assert all(args[1:3] == ["-I", "-c"] for args, _ in calls)
    assert all(options["env"] == {"SENTINEL": "kept"} for _, options in calls)


def test_windows_runtime_metadata_preserves_dependency_query_failure(monkeypatch):
    from types import SimpleNamespace

    module = harness()
    monkeypatch.setattr(module.sys, "platform", "win32")
    monkeypatch.setattr(
        module.subprocess,
        "run",
        lambda *args, **kwargs: SimpleNamespace(returncode=1, stdout="No module named 'rdflib'"),
    )
    with pytest.raises(RuntimeError, match="rdflib"):
        module.embedded_python_environment("C:/selected/python.exe", {})
