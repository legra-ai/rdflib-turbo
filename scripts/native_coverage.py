"""Measure native Rust execution from Rust tests and installed-wheel Python tests.

Use a separate target directory and venv. Never replace a development extension.
Reports retain Rust-only, Python-only, companion-only and merged observations.
"""

import argparse
import errno
import gzip
import hashlib
import importlib
import importlib.util
import json
import os
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import tomllib
import uuid
import zipfile
from contextlib import contextmanager
from datetime import UTC, datetime
from email.parser import Parser
from pathlib import Path

METRICS = ("lines", "regions", "functions")
TOOL_VERSION = "0.9.1"


def source_authority(root, archive, archive_sha256, *, project="rdflib-turbo"):
    """Keep immutable Git and published original-archive authority distinct."""
    if (archive is None) != (archive_sha256 is None):
        raise ValueError("Both source archive and original SHA-256 are required")
    if archive is None:
        return {
            "kind": "git",
            "revision": run(["git", "-C", root, "rev-parse", "HEAD"], capture=True).strip(),
        }
    spec = importlib.util.spec_from_file_location(
        "coverage_archive", Path(__file__).with_name("coverage_archive.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.freeze(Path(archive), root, archive_sha256, project=project)


def run(arguments, environment=None, *, capture=False):
    """Use posix_spawn on Linux/macOS, never fork after native initialization."""
    arguments = list(map(str, arguments))
    environment = dict(os.environ) if environment is None else environment
    executable = shutil.which(arguments[0], path=environment.get("PATH"))
    if executable is None:
        raise FileNotFoundError(arguments[0])
    if sys.platform not in {"darwin", "linux"}:
        raise RuntimeError("Native coverage currently supports Linux and macOS")
    print("+ " + shlex.join(arguments), file=sys.stderr, flush=True)
    with tempfile.TemporaryFile() as output:
        actions = [(os.POSIX_SPAWN_DUP2, output.fileno(), 1)] if capture else []
        for entry in os.listdir("/dev/fd"):
            fd = int(entry)
            if fd < 3:
                continue
            try:
                inherited = os.get_inheritable(fd)
            except OSError as error:
                if error.errno == errno.EBADF:
                    continue  # The directory iterator has closed its descriptor.
                raise
            if inherited:
                actions.append((os.POSIX_SPAWN_CLOSE, fd))
        pid = os.posix_spawn(
            executable, arguments, environment, file_actions=actions, setsigdef=(signal.SIGPIPE,)
        )
        _, status = os.waitpid(pid, 0)
        code = os.waitstatus_to_exitcode(status)
        output.seek(0)
        text = output.read().decode()
        if code:
            raise RuntimeError(f"Command failed ({code}): {shlex.join(arguments)}\n{text}")
        return text


def runtime_query(python, code, environment):
    """Query the selected runtime portably without extending POSIX coverage support."""
    arguments = [python, "-I", "-c", code]
    if sys.platform != "win32":
        return run(arguments, environment, capture=True)
    # Windows uses CreateProcess; the native coverage collector remains POSIX-only.
    result = subprocess.run(
        list(map(str, arguments)),
        env=environment,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    if result.returncode:
        raise RuntimeError(f"Runtime query failed ({result.returncode}): {result.stdout}")
    return result.stdout


def embedded_python_environment(python, environment):
    """Bind embedded CPython to the selected runtime and its installed dependencies."""
    prefix = runtime_query(python, "import sys; print(sys.base_prefix)", environment).strip()
    if not Path(prefix).is_absolute():
        raise RuntimeError("Python did not report an absolute runtime prefix")
    installed = json.loads(
        runtime_query(
            python,
            "import json, sysconfig, rdflib; "
            "print(json.dumps({'purelib': sysconfig.get_path('purelib'), "
            "'platlib': sysconfig.get_path('platlib'), 'rdflib': rdflib.__file__}))",
            environment,
        )
    )
    sites = []
    for name in ("purelib", "platlib"):
        value = installed.get(name)
        if not isinstance(value, str) or not value or not Path(value).is_absolute():
            raise RuntimeError(f"Python did not report an absolute {name} directory")
        if value not in sites:
            sites.append(value)
    origin = installed.get("rdflib")
    if (
        not isinstance(origin, str)
        or not Path(origin).is_absolute()
        or not any(Path(origin).resolve().is_relative_to(Path(site).resolve()) for site in sites)
    ):
        raise RuntimeError("Selected Python imported rdflib outside its installed site directories")
    configured = {
        **environment,
        "PYTHONHOME": prefix,
        "PYTHONPATH": os.pathsep.join(sites),
        "PYTHONNOUSERSITE": "1",
        "PYTHONSAFEPATH": "1",
    }
    if sys.platform == "linux":
        library = runtime_query(
            python, "import sysconfig; print(sysconfig.get_config_var('LIBDIR'))", environment
        ).strip()
        if not Path(library).is_absolute():
            raise RuntimeError("Python did not report an absolute library directory")
        previous = environment.get("LD_LIBRARY_PATH")
        configured["LD_LIBRARY_PATH"] = library + (os.pathsep + previous if previous else "")
    return configured


def initialize_project(project, python, environment):
    """Bare uv projects need a separate interpreter pin, not just a lower bound."""
    commands = [
        ["uv", "init", "--bare", "--no-workspace", "--vcs", "none", "--python", python, project],
        ["uv", "python", "pin", "--project", project, python],
    ]
    for arguments in commands:
        if sys.platform == "win32":
            # Windows project setup uses CreateProcess; LLVM instrumentation is still POSIX-only.
            subprocess.run(list(map(str, arguments)), env=environment, check=True)
        else:
            run(arguments, environment)


def initialize_dependencies(root, profiles, output, project, python, environment, yaml_wheel):
    """Export authoritative locked dependencies before creating an isolated project."""
    root, profiles, output = root.resolve(), profiles.resolve(), output.resolve()
    selections = {"rdflib-turbo": ["--group", "coverage"], "pyshacl-turbo": ["--extra", "test"]}
    selection = selections[project]
    omitted = (
        ["--no-emit-package", "pyshacl-turbo", "--no-emit-package", "owlrl-turbo"]
        if project == "rdflib-turbo"
        else []
    )
    sources = {name: (root / name).read_bytes() for name in ("pyproject.toml", "uv.lock")}
    if tomllib.loads(sources["pyproject.toml"].decode())["project"]["name"] != project:
        raise ValueError("Coverage project identity does not match source metadata")
    profiles.mkdir(parents=True, exist_ok=True)
    lock = profiles / "requirements.lock"
    run(
        [
            "uv",
            "export",
            "--project",
            root,
            "--locked",
            *selection,
            "--no-dev",
            "--no-emit-project",
            *omitted,
            "--no-hashes",
            "--output-file",
            lock,
        ],
        environment,
    )
    preserved = output / "dependencies"
    preserved.mkdir(parents=True, exist_ok=True)
    for name, content in sources.items():
        if (root / name).read_bytes() != content:
            raise RuntimeError(f"Coverage dependency input changed during export: {name}")
        (preserved / name).write_bytes(content)
    (preserved / "requirements.lock").write_bytes(lock.read_bytes())
    provenance = {
        "source_pyproject_sha256": digest(preserved / "pyproject.toml"),
        "source_uv_lock_sha256": digest(preserved / "uv.lock"),
        "export_sha256": digest(lock),
        "export_selection": selection,
        "uv_version": run(["uv", "--version"], environment, capture=True).strip(),
    }
    project_environment = profiles / "project"
    initialize_project(project_environment, python, environment)
    run(
        [
            "uv",
            "add",
            "--project",
            project_environment,
            "--no-sync",
            "--requirements",
            lock,
            yaml_wheel,
        ],
        environment,
    )
    run(["uv", "sync", "--project", project_environment, "--locked", "--no-build"], environment)
    verify_dependency_inputs(root, profiles, output, provenance)
    return provenance


@contextmanager
def locked_python_environment(root, output, python, yaml_authority):
    """Bootstrap native source-test callers from exact locked dependency inputs."""
    root, output = Path(root).resolve(), Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    profiles = output / "profiles"
    authority = Path(yaml_authority).resolve(strict=True)
    checker = yaml_checker()
    frozen = checker.verify(root, authority)
    environment = dict(os.environ)
    for name in (
        "VIRTUAL_ENV",
        "UV_PYTHON",
        "UV_PROJECT_ENVIRONMENT",
        "UV_PROJECT",
        "PYTHONPATH",
        "PYTHONHOME",
        "PYO3_PYTHON",
    ):
        environment.pop(name, None)
    dependencies = initialize_dependencies(
        root, profiles, output, "rdflib-turbo", python, environment, frozen["wheel"]
    )
    if checker.verify(root, authority) != frozen:
        raise RuntimeError("YAML dependency changed during source-test dependency setup")
    project = profiles / "project"
    venv = project / ".venv"
    selected = venv / ("Scripts/python.exe" if sys.platform == "win32" else "bin/python")
    project_run = ["uv", "run", "--project", project, "--locked", "--no-sync", "--no-build"]
    installed = verify_installed_yaml(root, authority, project_run, venv, environment)
    configured = embedded_python_environment(selected, environment)
    configured["PYO3_PYTHON"] = str(selected)
    (output / "runtime-inputs.json").write_text(
        json.dumps(
            {
                "python": str(selected),
                "dependencies": dependencies,
                "yaml_inputs": frozen["record"],
                "yaml_installed": installed,
            },
            indent=2,
        )
        + "\n"
    )
    try:
        yield configured
    finally:
        verify_dependency_inputs(root, profiles, output, dependencies)
        if checker.verify(root, authority) != frozen:
            raise RuntimeError("YAML dependency changed during native source tests")
        if verify_installed_yaml(root, authority, project_run, venv, environment) != installed:
            raise RuntimeError("Installed YAML changed during native source tests")


def facade_payload(wheel):
    """Require a universal facade and retain hashes for every package payload."""
    with zipfile.ZipFile(wheel) as archive:
        names = archive.namelist()
        metadata = [name for name in names if name.endswith(".dist-info/WHEEL")]
        if len(metadata) != 1:
            raise RuntimeError("Expected one pure facade wheel metadata record")
        fields = Parser().parsestr(archive.read(metadata[0]).decode())
        if fields.get("Root-Is-Purelib") != "true" or fields.get_all("Tag") != ["py3-none-any"]:
            raise RuntimeError("SHACL facade wheel must be pure and universal")
        if any(re.search(r"\.(?:so(?:\.[^/]*)?|pyd|dylib|dll)$", name) for name in names):
            raise RuntimeError("SHACL facade wheel contains a second native runtime")
        payload = {
            name: hashlib.sha256(archive.read(name)).hexdigest()
            for name in names
            if name.startswith("pyshacl_turbo/") and not name.endswith("/")
        }
        if not {"pyshacl_turbo/__init__.py", "pyshacl_turbo/_native.py"}.issubset(payload):
            raise RuntimeError("SHACL facade wheel is missing its Python binding facade")
        return payload


def loaded_facade_record():
    """Inspect the real loaded Python facade and both native class owners."""
    package = importlib.import_module("pyshacl_turbo")
    facade = importlib.import_module("pyshacl_turbo._native")
    provider = importlib.import_module("rdflib_turbo._native")
    if (
        facade.CompiledShapes is not provider.shacl.CompiledShapes
        or facade.TargetData is not provider.shacl.TargetData
    ):
        raise RuntimeError("SHACL facade uses another native provider")
    directory = Path(package.__file__).resolve().parent
    return {
        "package": str(directory),
        "payload": {
            "pyshacl_turbo/" + path.relative_to(directory).as_posix(): digest(path)
            for path in directory.rglob("*")
            if path.is_file() and "__pycache__" not in path.parts and path.suffix != ".pyc"
        },
        "engine_fingerprint": provider.shacl.SHACL_ENGINE_FINGERPRINT,
    }


def candidate_facade_authority(source, wheel, candidate):
    """Use the candidate's original archive rather than fabricating Git authority."""
    spec = importlib.util.spec_from_file_location(
        "facade_candidate", Path(__file__).with_name("facade_candidate.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.freeze_source(source, wheel, payload_reader=facade_payload, **candidate)


def candidate_parameters(authority, receipt, base_revision, provider_version):
    """Keep explicit test-only candidates separate from published package authority."""
    if (receipt is None) != (base_revision is None):
        raise ValueError("Both candidate receipt and base revision are required")
    if receipt is None:
        return None
    if authority["kind"] != "git" or not re.fullmatch(r"[0-9a-f]{40}", authority["revision"]):
        raise ValueError("Facade candidates require an immutable Git provider")
    if not re.fullmatch(r"[0-9a-f]{40}", base_revision):
        raise ValueError("An immutable facade base revision is required")
    return {
        "receipt": Path(receipt),
        "facade_revision": base_revision,
        "provider_revision": authority["revision"],
        "provider_version": provider_version,
    }


def facade_source_payload(source, wheel):
    """Verify original facade source/wheel package bytes without process launches."""
    package = source.resolve(strict=True) / "python/pyshacl_turbo"
    payload = {}
    for path in package.rglob("*"):
        if not path.is_file() or "__pycache__" in path.parts or path.suffix == ".pyc":
            continue
        if not path.resolve(strict=True).is_relative_to(package.resolve()):
            raise RuntimeError("SHACL source package payload points outside its package")
        payload["pyshacl_turbo/" + path.relative_to(package).as_posix()] = digest(path)
    if payload != facade_payload(wheel):
        raise RuntimeError("SHACL source package payload differs from the explicit wheel")
    return payload


def release_project_helper():
    spec = importlib.util.spec_from_file_location(
        "facade_release_project", Path(__file__).with_name("facade_release_project.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def run_facade_suite(arguments, environment, authority, frozen, candidate):
    stage_environment = dict(environment)
    for name in (
        "SHACL_RELEASE_PROJECT_AUTHORITY",
        "SHACL_BASE_REVISION",
        "SHACL_CANDIDATE_RECEIPT",
    ):
        stage_environment.pop(name, None)
    with release_project_helper().stage(authority, frozen) as original:
        stage_environment.update(original)
        if candidate is not None:
            stage_environment["SHACL_BASE_REVISION"] = candidate["facade_revision"]
            stage_environment["SHACL_CANDIDATE_RECEIPT"] = str(candidate["receipt"])
        run(arguments, stage_environment)


def freeze_facade_inputs(source, wheel, *, candidate=None):
    """Freeze acceptance sources separately from byte-equal runtime wheel payload."""
    source, wheel = source.resolve(strict=True), wheel.resolve(strict=True)
    manifest = tomllib.loads((source / "pyproject.toml").read_text())
    if manifest["project"]["name"] != "pyshacl-turbo" or not (source / "tests").is_dir():
        raise ValueError("SHACL coverage requires the facade acceptance source")
    authority = candidate_facade_authority(source, wheel, candidate) if candidate else None
    if authority:
        revision = "candidate-sdist-sha256:" + authority["sha256"]
        source_hashes = authority["files"]
    else:
        if run(
            ["git", "-C", source, "status", "--porcelain", "--untracked-files=all"], capture=True
        ):
            raise RuntimeError("SHACL acceptance source must be a clean frozen checkout")
        revision = run(["git", "-C", source, "rev-parse", "HEAD"], capture=True).strip()
        tracked = run(["git", "-C", source, "ls-files", "-z"], capture=True).split("\0")
        source_hashes = {}
        for name in filter(None, tracked):
            path = source / name
            if not path.resolve(strict=True).is_relative_to(source):
                raise RuntimeError("SHACL acceptance source points outside its frozen checkout")
            source_hashes[name] = digest(path)
    payload = facade_source_payload(source, wheel)
    return {
        "revision": revision,
        "source_sha256": source_hashes,
        "package_payload": payload,
        "wheel_sha256": digest(wheel),
        **({"source_authority": authority} if authority else {}),
    }


def verify_facade_inputs(source, wheel, frozen, *, candidate=None):
    """Reject changed tests, source inputs or wheel bytes during measurement."""
    if freeze_facade_inputs(source, wheel, candidate=candidate) != frozen:
        raise RuntimeError("Frozen SHACL acceptance source or explicit wheel changed")


def verify_facade_record(wheel, actual, venv):
    """Compare a loaded record with every original pure-wheel package byte."""
    expected = facade_payload(wheel)
    if not Path(actual["package"]).is_relative_to(venv.resolve()):
        raise RuntimeError("SHACL facade loaded outside the isolated coverage environment")
    if actual["payload"] != expected:
        raise RuntimeError("Installed SHACL facade payload differs from its wheel")
    return {**actual, "wheel_sha256": digest(wheel)}


def verify_loaded_facade(wheel, venv):
    """Portable in-process verifier for an isolated wheel interpreter."""
    return verify_facade_record(wheel, loaded_facade_record(), venv)


def verify_installed_facade(wheel, project_run, venv, environment):
    """Attest installed bytes and classes without importing a source facade."""
    probe = """
import importlib.util, json, sys
spec = importlib.util.spec_from_file_location('installed_facade_checker', sys.argv[1])
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)
print(json.dumps(checker.loaded_facade_record()))
"""
    actual = json.loads(
        run(
            [*project_run, "python", "-I", "-c", probe, Path(__file__).resolve()],
            environment,
            capture=True,
        )
    )
    return verify_facade_record(wheel, actual, venv)


def companion_checker():
    spec = importlib.util.spec_from_file_location(
        "coverage_companion_checker", Path(__file__).with_name("pure_companion.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def freeze_owl_inputs(source, wheel):
    if wheel is None or not Path(wheel).is_file():
        raise ValueError("Coverage requires the original OWL companion wheel")
    return {
        "wheel_sha256": digest(wheel),
        "package_payload": companion_checker().verify_payload(source, wheel),
    }


def yaml_checker():
    spec = importlib.util.spec_from_file_location(
        "coverage_yaml_checker", Path(__file__).with_name("yaml_test_inputs.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def verify_installed_yaml(owner, authority, project_run, venv, environment):
    """Verify the original dependency and its complete isolated installed payload."""
    frozen = yaml_checker().verify(owner, authority)
    actual = json.loads(
        run(
            [
                *project_run,
                "python",
                "-I",
                "-B",
                str(Path(__file__).with_name("yaml_test_inputs.py")),
                "--wheel",
                str(frozen["wheel"]),
                "--environment",
                str(venv),
            ],
            environment,
            capture=True,
        )
    )
    if yaml_checker().verify(owner, authority) != frozen:
        raise RuntimeError("YAML dependency authority changed during its isolated probe")
    return actual


def install_python_cohort(
    project, native_wheel, facade_wheel, owlrl_wheel, yaml_wheel, environment
):
    installation = [
        wheel
        for wheel in (native_wheel, facade_wheel, owlrl_wheel, yaml_wheel)
        if wheel is not None
    ]
    run(["uv", "add", "--project", project, "--no-sync", *installation], environment)
    run(["uv", "sync", "--project", project, "--locked", "--no-build"], environment)


def verify_installed_companion(source, wheel, frozen, project_run, venv, environment):
    if freeze_owl_inputs(source, wheel) != frozen:
        raise RuntimeError("Original OWL companion inputs changed during coverage")
    actual = json.loads(
        run(
            [
                *project_run,
                "python",
                "-I",
                "-B",
                str(Path(__file__).with_name("pure_companion.py")),
                "--wheel",
                str(wheel),
                "--environment",
                str(venv),
            ],
            environment,
            capture=True,
        )
    )
    if (
        actual["wheel_sha256"] != frozen["wheel_sha256"]
        or actual["payload"] != frozen["package_payload"]
    ):
        raise RuntimeError("Installed OWL companion differs from its original wheel")
    return actual


def verify_dependency_inputs(root, profiles, output, provenance):
    """Never attribute a measurement to source/export bytes that changed mid-run."""
    checks = {
        root / "pyproject.toml": provenance["source_pyproject_sha256"],
        root / "uv.lock": provenance["source_uv_lock_sha256"],
        profiles / "requirements.lock": provenance["export_sha256"],
        output / "dependencies/pyproject.toml": provenance["source_pyproject_sha256"],
        output / "dependencies/uv.lock": provenance["source_uv_lock_sha256"],
        output / "dependencies/requirements.lock": provenance["export_sha256"],
    }
    for path, expected in checks.items():
        if digest(path) != expected:
            raise RuntimeError(f"Coverage dependency input changed: {path}")


def record_environment_lock(project, output, stage):
    """Preserve the actual resolved environment used by each execution stage."""
    destination = output / "dependencies" / f"{stage}-uv.lock"
    destination.write_bytes((project / "uv.lock").read_bytes())
    return digest(destination)


def parse_environment(text):
    result = {}
    for line in text.splitlines():
        words = shlex.split(line)
        if len(words) != 1 or not re.fullmatch(r"[A-Z_][A-Z0-9_]*=.*", words[0]):
            raise ValueError(f"Invalid coverage environment assignment: {line!r}")
        name, value = words[0].split("=", 1)
        result[name] = value
    flags = result.get("RUSTFLAGS", "") + result.get("__CARGO_LLVM_COV_RUSTC_WRAPPER_RUSTFLAGS", "")
    if "instrument-coverage" not in flags:
        raise ValueError("Coverage instrumentation is missing")
    return result


def owned_sources(root):
    candidates = list((root / "src").rglob("*.rs"))
    candidates += list((root / "crates").glob("*/src/**/*.rs"))
    return {
        path.resolve().relative_to(root).as_posix()
        for path in candidates
        if not path.name.endswith(("_tests.rs", "-tests.rs")) and path.name != "tests.rs"
    }


def summarize(report, root):
    root = root.resolve()
    expected = owned_sources(root)
    inventory = root / "native-coverage-sources.json"
    unmapped = json.loads(inventory.read_text()) if inventory.exists() else {}
    for name, entry in unmapped.items():
        if name not in expected or digest(root / name) != entry["sha256"]:
            raise RuntimeError(f"Unmapped source hash changed: {name}")
    files, vendored = {}, {}
    for block in report["data"]:
        for row in block["files"]:
            path = Path(row["filename"]).resolve()
            if not path.is_relative_to(root):
                continue
            name = path.relative_to(root).as_posix()
            metrics = {
                key: {part: row["summary"][key][part] for part in ("count", "covered")}
                for key in METRICS
            }
            if name in expected:
                files[name] = metrics
            elif (
                name.startswith("vendor/")
                and "/tests/" not in name
                and not path.name.endswith("_tests.rs")
            ):
                vendored[name] = metrics
    if unmapped.keys() & files.keys():
        raise RuntimeError(
            "A previously unmapped source now has executable coverage; update its inventory"
        )
    if not expected or expected - files.keys() - unmapped.keys():
        raise RuntimeError(
            f"Owned Rust files missing from coverage: {sorted(expected - files.keys() - unmapped.keys())}"
        )

    def total(rows):
        result = {}
        for metric in METRICS:
            count = sum(row[metric]["count"] for row in rows.values())
            covered = sum(row[metric]["covered"] for row in rows.values())
            result[metric] = {
                "count": count,
                "covered": covered,
                "percent": covered * 100 / count if count else None,
            }
        return result

    return {
        "owned": total(files),
        "vendored": total(vendored),
        "files": files,
        "vendored_files": vendored,
        "branches": None,
        "unmapped_non_executable": unmapped,
    }


def verify_denominators(actual, empty):
    if actual["files"].keys() != empty["files"].keys() or any(
        actual["files"][name][metric]["count"] != empty["files"][name][metric]["count"]
        for name in empty["files"]
        for metric in METRICS
    ):
        raise RuntimeError("Runtime profiles changed the owned-source coverage denominator")


def require_python_execution(summary, binding):
    if summary["files"].get(binding, {}).get("lines", {}).get("covered", 0) <= 0:
        raise RuntimeError("Python tests did not execute the instrumented Rust bindings")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def export(llvm, profiles, objects, output, root):
    raw = sorted(path for directory in profiles for path in directory.glob("*.profraw"))
    if not raw:
        raise RuntimeError(f"No runtime coverage profiles: {profiles}")
    profdata = output.with_suffix(".profdata")
    run([llvm / "llvm-profdata", "merge", "-sparse", *raw, "-o", profdata])
    flags = [f"-instr-profile={profdata}", objects[0]]
    for obj in objects[1:]:
        flags += ["-object", obj]
    payload = run([llvm / "llvm-cov", "export", "-skip-functions", *flags], capture=True)
    output.with_suffix(".llvm.json.gz").write_bytes(gzip.compress(payload.encode(), mtime=0))
    summary = summarize(json.loads(payload), root)
    empty = run(
        [llvm / "llvm-cov", "export", "-empty-profile", "-skip-functions", *flags[1:]], capture=True
    )
    output.with_suffix(".empty.llvm.json.gz").write_bytes(gzip.compress(empty.encode(), mtime=0))
    verify_denominators(summary, summarize(json.loads(empty), root))
    output.with_suffix(".json").write_text(json.dumps(summary, indent=2) + "\n")
    return summary, flags


def write_markdown(result, output):
    lines = [
        "# Native Rust coverage",
        "",
        f"Measured {result['date']}; Rust {result['rustc'].splitlines()[0]}.",
        "",
        "Line, region and function coverage of owned Rust source, including Python execution of the installed instrumented wheel. Branch coverage is not instrumented.",
        "",
        "| Suite | Lines | Regions | Functions |",
        "|---|---:|---:|---:|",
    ]
    for name, stage in result["stages"].items():

        def cell(metric, stage=stage):
            value = stage["owned"][metric]
            return (
                f"{value['covered']}/{value['count']} ({value['percent']:.2f}%)"
                if value["count"]
                else "unmeasured"
            )

        lines.append(f"| {name} | {cell('lines')} | {cell('regions')} | {cell('functions')} |")
    lines += [
        "",
        "Combined coverage merges execution counters; percentages are never added or averaged.",
        "Vendored code is reported separately in JSON. Registry dependencies, test source files and build scripts are outside the owned-source denominator.",
        "Inline test modules remain in LLVM's source mapping. These are finite execution measurements, not compatibility percentages.",
        "",
        f"Rust test cases discovered: {result['rust_test_cases']}.",
        "",
        "Per-file counts and provenance are in summary.json; raw LLVM exports, merged profiles and owned-source HTML are retained as artifacts. Dependency and vendored-patch coverage is outside this owned-source measurement.",
        "",
    ]
    output.write_text("\n".join(lines))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path("native-coverage"))
    parser.add_argument("--python", default="3.13")
    parser.add_argument("--yaml-authority", type=Path, required=True)
    parser.add_argument("--owlrl-source", type=Path)
    parser.add_argument("--owlrl-wheel", type=Path)
    parser.add_argument("--owlrl-archive", type=Path)
    parser.add_argument("--owlrl-sha256")
    parser.add_argument("--shacl-source", type=Path)
    parser.add_argument("--shacl-wheel", type=Path)
    parser.add_argument("--shacl-candidate-receipt", type=Path)
    parser.add_argument("--shacl-base-revision")
    parser.add_argument("--source-archive", type=Path)
    parser.add_argument("--source-sha256")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    os.chdir(root)
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    manifest = tomllib.loads((root / "pyproject.toml").read_text())
    project = manifest["project"]["name"]
    if project not in {"pyshacl-turbo", "rdflib-turbo"}:
        raise ValueError(f"Unsupported project: {project}")
    owlrl = args.owlrl_source.resolve() if args.owlrl_source else None
    owlrl_wheel = args.owlrl_wheel.resolve(strict=True) if args.owlrl_wheel else None
    shacl = args.shacl_source.resolve() if args.shacl_source else None
    facade_wheel = args.shacl_wheel.resolve(strict=True) if args.shacl_wheel else None
    facade_inputs = None
    release_project = None
    release_project_inputs = None
    candidate = None
    authority = None
    if project == "rdflib-turbo":
        if facade_wheel is None:
            raise ValueError(
                "Provider coverage requires --shacl-wheel with the exact pure artifact"
            )
        if shacl is None or not (shacl / "tests").is_dir():
            raise ValueError("Provider coverage requires --shacl-source with the facade test suite")
        facade_manifest = tomllib.loads((shacl / "pyproject.toml").read_text())
        if facade_manifest["project"]["name"] != "pyshacl-turbo":
            raise ValueError("SHACL coverage source has the wrong project identity")
        authority = source_authority(root, args.source_archive, args.source_sha256)
        candidate = candidate_parameters(
            authority,
            args.shacl_candidate_receipt,
            args.shacl_base_revision,
            manifest["project"]["version"],
        )
        facade_inputs = freeze_facade_inputs(shacl, facade_wheel, candidate=candidate)
        (output / "facade-inputs.json").write_text(json.dumps(facade_inputs, indent=2))
        if candidate is not None:
            release_helper = release_project_helper()
            release_project = release_helper.prepare(candidate, output, facade_payload)
            release_project_inputs = release_helper.verify(release_project)
    if project == "rdflib-turbo" and (owlrl is None or not (owlrl / "tests").is_dir()):
        raise ValueError(
            "rdflib-turbo coverage requires --owlrl-source with the companion test suite"
        )
    if authority is None:
        authority = source_authority(root, args.source_archive, args.source_sha256)
    owlrl_authority = None
    if owlrl:
        owlrl_authority = source_authority(
            owlrl, args.owlrl_archive, args.owlrl_sha256, project="owlrl-turbo"
        )
    owlrl_inputs = freeze_owl_inputs(owlrl, owlrl_wheel) if owlrl else None
    yaml_authority = args.yaml_authority.resolve(strict=True)
    yaml_inputs = yaml_checker().verify(root, yaml_authority)
    environment = dict(os.environ)
    environment.pop("PYTHONPATH", None)
    environment.pop("VIRTUAL_ENV", None)
    environment.pop("UV_PYTHON", None)
    environment.pop("SHACL_RELEASE_PROJECT_AUTHORITY", None)
    environment.pop("UV_PROJECT_ENVIRONMENT", None)
    environment.pop("UV_PROJECT", None)
    if (
        run(["cargo", "llvm-cov", "--version"], environment, capture=True).strip()
        != f"cargo-llvm-cov {TOOL_VERSION}"
    ):
        raise RuntimeError(f"Install cargo-llvm-cov {TOOL_VERSION}")
    target = root / "target/native-coverage"
    environment["CARGO_TARGET_DIR"] = str(target / "build")
    environment.update(
        parse_environment(
            run(["cargo", "llvm-cov", "show-env", "--no-cfg-coverage"], environment, capture=True)
        )
    )
    rustc = run(["rustc", "-vV"], environment, capture=True)
    sysroot = Path(run(["rustc", "--print", "sysroot"], environment, capture=True).strip())
    host = next(line.split(": ", 1)[1] for line in rustc.splitlines() if line.startswith("host: "))
    llvm = sysroot / "lib/rustlib" / host / "bin"
    for name in ("llvm-cov", "llvm-profdata"):
        if not (llvm / name).is_file():
            raise FileNotFoundError(
                f"Install llvm-tools-preview for this Rust toolchain: {llvm / name}"
            )
    profiles = target / "profiles" / uuid.uuid4().hex
    for name in ("build", "rust", "python", "shacl", "owlrl"):
        (profiles / name).mkdir(parents=True)
    environment["LLVM_PROFILE_FILE"] = str(profiles / "build/%p-%m.profraw")
    project_environment = profiles / "project"
    dependencies = initialize_dependencies(
        root, profiles, output, project, args.python, environment, yaml_inputs["wheel"]
    )
    if yaml_checker().verify(root, yaml_authority) != yaml_inputs:
        raise RuntimeError("YAML dependency changed during coverage dependency setup")
    venv = project_environment / ".venv"
    python = venv / "bin/python"
    rust_lock = record_environment_lock(project_environment, output, "rust")
    project_run = ["uv", "run", "--project", project_environment, "--locked", "--no-sync"]
    environment["PYO3_PYTHON"] = str(python)
    artifacts = run(
        ["cargo", "test", "--workspace", "--locked", "--no-run", "--message-format=json"],
        environment,
        capture=True,
    )
    executables = sorted(
        {
            row["executable"]
            for line in artifacts.splitlines()
            if line.startswith("{")
            for row in [json.loads(line)]
            if row.get("executable")
        }
    )
    if not executables:
        raise RuntimeError("Cargo did not produce Rust test executables")
    rust_environment = embedded_python_environment(python, environment)
    rust_cases = 0
    for executable in executables:
        listing = run([executable, "--list"], environment, capture=True)
        rust_cases += sum(line.endswith(": test") for line in listing.splitlines())
        run(
            [executable],
            {**rust_environment, "LLVM_PROFILE_FILE": str(profiles / "rust/%p-%m.profraw")},
        )
    wheels = target / "wheels" / uuid.uuid4().hex
    run(
        [
            "uv",
            "build",
            "--wheel",
            "--python",
            python,
            "--out-dir",
            wheels,
            "--config-setting",
            "maturin.build-args=--locked --profile dev",
        ],
        environment,
    )
    wheel_files = list(wheels.glob("*.whl"))
    if len(wheel_files) != 1:
        raise RuntimeError("Expected exactly one instrumented wheel")
    wheel = wheel_files[0]
    if shacl:
        verify_facade_inputs(shacl, facade_wheel, facade_inputs, candidate=candidate)
    if owlrl and freeze_owl_inputs(owlrl, owlrl_wheel) != owlrl_inputs:
        raise RuntimeError("Original OWL companion inputs changed during the native build")
    if yaml_checker().verify(root, yaml_authority) != yaml_inputs:
        raise RuntimeError("YAML dependency changed during the native build")
    install_python_cohort(
        project_environment, wheel, facade_wheel, owlrl_wheel, yaml_inputs["wheel"], environment
    )
    yaml_installed = verify_installed_yaml(root, yaml_authority, project_run, venv, environment)
    if owlrl:
        owlrl_installed = verify_installed_companion(
            owlrl, owlrl_wheel, owlrl_inputs, project_run, venv, environment
        )
    module = project.replace("-", "_") + "._native"
    loaded = Path(
        run(
            [
                *project_run,
                "python",
                "-I",
                "-c",
                f"import importlib; print(importlib.import_module({module!r}).__file__)",
            ],
            environment,
            capture=True,
        ).strip()
    )
    if not loaded.resolve().is_relative_to(venv.resolve()):
        raise RuntimeError(f"Python loaded an extension outside the coverage venv: {loaded}")
    with zipfile.ZipFile(wheel) as archive:
        member = next(name for name in archive.namelist() if name.endswith((".so", ".pyd")))
        if hashlib.sha256(archive.read(member)).hexdigest() != digest(loaded):
            raise RuntimeError("Loaded native extension does not match the instrumented wheel")
    objects = [str(loaded), *executables]
    result = {
        "project": project,
        "date": datetime.now(UTC).isoformat(),
        "rustc": rustc,
        "cargo_llvm_cov": TOOL_VERSION,
        "revision": (
            authority["revision"]
            if authority["kind"] == "git"
            else "sdist-sha256:" + authority["sha256"]
        ),
        "source_authority": authority,
        "source_sha256": {name: digest(root / name) for name in sorted(owned_sources(root))},
        "harness_sha256": digest(Path(__file__)),
        "test_lock_sha256": dependencies["export_sha256"],
        "dependencies": dependencies,
        "wheel_sha256": digest(wheel),
        "native_sha256": digest(loaded),
        "rust_test_cases": rust_cases,
        "stages": {},
    }
    result["stages"]["rust"], _ = export(llvm, [profiles / "rust"], objects, output / "rust", root)
    result["stages"]["rust"]["environment_lock_sha256"] = rust_lock
    python_lock = record_environment_lock(project_environment, output, "python")
    run(
        [
            *project_run,
            "python",
            "-I",
            "-m",
            "pytest",
            root / "tests",
            "-q",
            "--cov=" + project.replace("-", "_"),
            "--cov-branch",
            "--cov-fail-under=100",
            f"--cov-report=json:{output / 'python-coverage.json'}",
            f"--cov-report=html:{output / 'python-html'}",
        ],
        {**environment, "LLVM_PROFILE_FILE": str(profiles / "python/%p-%m.profraw")},
    )
    binding = "crates/python/src/lib.rs" if project == "pyshacl-turbo" else "src/lib.rs"
    result["stages"]["python"], _ = export(
        llvm, [profiles / "python"], objects, output / "python", root
    )
    result["yaml_inputs"] = yaml_inputs["record"]
    result["yaml_installed"] = yaml_installed
    result["yaml_environment"] = {"project": str(project_environment), "venv": str(venv)}
    result["stages"]["python"]["environment_lock_sha256"] = python_lock
    require_python_execution(result["stages"]["python"], binding)
    if owlrl:
        verify_installed_companion(owlrl, owlrl_wheel, owlrl_inputs, project_run, venv, environment)
    stages = [profiles / "rust", profiles / "python"]
    if shacl:
        result["shacl_inputs"] = facade_inputs
        result["shacl_facade"] = verify_installed_facade(
            facade_wheel, project_run, venv, environment
        )
        result["shacl_revision"] = facade_inputs.get("source_authority", {}).get(
            "base_revision", facade_inputs["revision"]
        )
        shacl_lock = record_environment_lock(project_environment, output, "shacl")
        run_facade_suite(
            [
                *project_run,
                "python",
                "-I",
                "-m",
                "pytest",
                shacl / "tests",
                "-q",
                "--cov=pyshacl_turbo",
                "--cov-branch",
                "--cov-fail-under=100",
                f"--cov-report=json:{output / 'shacl-python-coverage.json'}",
                f"--cov-report=html:{output / 'shacl-python-html'}",
            ],
            {**environment, "LLVM_PROFILE_FILE": str(profiles / "shacl/%p-%m.profraw")},
            release_project,
            release_project_inputs,
            candidate,
        )
        if release_project_inputs is not None:
            result["shacl_release_project"] = release_project_inputs
        result["stages"]["shacl"], _ = export(
            llvm, [profiles / "shacl"], objects, output / "shacl", root
        )
        result["stages"]["shacl"]["environment_lock_sha256"] = shacl_lock
        require_python_execution(result["stages"]["shacl"], "crates/python/src/lib.rs")
        if owlrl:
            verify_installed_companion(
                owlrl, owlrl_wheel, owlrl_inputs, project_run, venv, environment
            )
        stages.append(profiles / "shacl")
    if owlrl:
        owlrl_lock = record_environment_lock(project_environment, output, "owlrl")
        result["owlrl_wheel_sha256"] = owlrl_inputs["wheel_sha256"]
        result["owlrl_inputs"] = owlrl_inputs
        result["owlrl_installed"] = owlrl_installed
        run(
            [
                *project_run,
                "python",
                "-I",
                "-m",
                "pytest",
                owlrl / "tests",
                "-q",
                "--cov=owlrl_turbo",
                "--cov-branch",
                "--cov-fail-under=100",
                f"--cov-report=json:{output / 'owlrl-python-coverage.json'}",
                f"--cov-report=html:{output / 'owlrl-python-html'}",
            ],
            {**environment, "LLVM_PROFILE_FILE": str(profiles / "owlrl/%p-%m.profraw")},
        )
        result["owlrl_source_authority"] = owlrl_authority
        result["owlrl_revision"] = (
            owlrl_authority["revision"]
            if owlrl_authority["kind"] == "git"
            else "sdist-sha256:" + owlrl_authority["sha256"]
        )
        result["stages"]["owlrl"], _ = export(
            llvm, [profiles / "owlrl"], objects, output / "owlrl", root
        )
        result["stages"]["owlrl"]["environment_lock_sha256"] = owlrl_lock
        require_python_execution(result["stages"]["owlrl"], binding)
        verify_installed_companion(owlrl, owlrl_wheel, owlrl_inputs, project_run, venv, environment)
        stages.append(profiles / "owlrl")
    result["stages"]["combined"], flags = export(llvm, stages, objects, output / "combined", root)
    run(
        [
            llvm / "llvm-cov",
            "show",
            "-format=html",
            f"-output-dir={output / 'html'}",
            *flags,
            *[root / name for name in sorted(owned_sources(root))],
        ],
        capture=True,
    )
    if (
        verify_installed_yaml(root, yaml_authority, project_run, venv, environment)
        != yaml_installed
    ):
        raise RuntimeError("Installed YAML dependency changed during coverage")
    verify_dependency_inputs(root, profiles, output, dependencies)
    if source_authority(root, args.source_archive, args.source_sha256) != authority:
        raise RuntimeError("Native provider source authority changed during measurement")
    if (
        owlrl
        and source_authority(owlrl, args.owlrl_archive, args.owlrl_sha256, project="owlrl-turbo")
        != owlrl_authority
    ):
        raise RuntimeError("OWL-RL source authority changed during measurement")
    if shacl:
        verify_facade_inputs(shacl, facade_wheel, facade_inputs, candidate=candidate)
    result["environment_lock_sha256"] = digest(project_environment / "uv.lock")
    (output / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    write_markdown(result, output / "summary.md")
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as destination:
            destination.write((output / "summary.md").read_text())
    print((output / "summary.md").read_text())


if __name__ == "__main__":
    main()
