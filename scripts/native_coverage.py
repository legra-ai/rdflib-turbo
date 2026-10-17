"""Measure native Rust execution from Rust tests and installed-wheel Python tests.

Use a separate target directory and venv. Never replace a development extension.
Reports retain Rust-only, Python-only, companion-only and merged observations.
"""

import argparse
import errno
import gzip
import hashlib
import json
import os
import re
import shlex
import shutil
import signal
import sys
import tempfile
import tomllib
import uuid
import zipfile
from datetime import UTC, datetime
from pathlib import Path

METRICS = ("lines", "regions", "functions")
TOOL_VERSION = "0.9.1"


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


def embedded_python_environment(python, environment):
    """An embedded interpreter needs its selected distribution's standard library."""
    prefix = run(
        [python, "-I", "-c", "import sys; print(sys.base_prefix)"], environment, capture=True
    ).strip()
    if not Path(prefix).is_absolute():
        raise RuntimeError("Python did not report an absolute runtime prefix")
    return {**environment, "PYTHONHOME": prefix}


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
    parser.add_argument("--owlrl-source", type=Path)
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
    if project == "rdflib-turbo" and (owlrl is None or not (owlrl / "tests").is_dir()):
        raise ValueError(
            "rdflib-turbo coverage requires --owlrl-source with the companion test suite"
        )
    environment = dict(os.environ)
    environment.pop("PYTHONPATH", None)
    environment.pop("VIRTUAL_ENV", None)
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
    for name in ("build", "rust", "python", "owlrl"):
        (profiles / name).mkdir(parents=True)
    environment["LLVM_PROFILE_FILE"] = str(profiles / "build/%p-%m.profraw")
    venv = profiles / "venv"
    run(["uv", "venv", "--python", args.python, venv], environment)
    python = venv / "bin/python"
    lock = root / (
        "requirements-test.lock" if project == "pyshacl-turbo" else "requirements-coverage.lock"
    )
    run(["uv", "pip", "install", "--python", python, "-r", lock, "maturin==1.15.0"], environment)
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
        [venv / "bin/maturin", "build", "--locked", "--interpreter", python, "--out", wheels],
        environment,
    )
    wheel_files = list(wheels.glob("*.whl"))
    if len(wheel_files) != 1:
        raise RuntimeError("Expected exactly one instrumented wheel")
    wheel = wheel_files[0]
    run(["uv", "pip", "install", "--python", python, "--no-deps", wheel], environment)
    module = project.replace("-", "_") + "._native"
    loaded = Path(
        run(
            [
                python,
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
        "revision": run(["git", "rev-parse", "HEAD"], capture=True).strip(),
        "source_sha256": {name: digest(root / name) for name in sorted(owned_sources(root))},
        "harness_sha256": digest(Path(__file__)),
        "test_lock_sha256": digest(lock),
        "wheel_sha256": digest(wheel),
        "native_sha256": digest(loaded),
        "rust_test_cases": rust_cases,
        "stages": {},
    }
    result["stages"]["rust"], _ = export(llvm, [profiles / "rust"], objects, output / "rust", root)
    run(
        [
            python,
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
    require_python_execution(result["stages"]["python"], binding)
    stages = [profiles / "rust", profiles / "python"]
    if owlrl:
        run(["uv", "pip", "install", "--python", python, "--no-deps", owlrl], environment)
        run(
            [
                python,
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
        result["owlrl_revision"] = run(
            ["git", "-C", owlrl, "rev-parse", "HEAD"], capture=True
        ).strip()
        result["stages"]["owlrl"], _ = export(
            llvm, [profiles / "owlrl"], objects, output / "owlrl", root
        )
        require_python_execution(result["stages"]["owlrl"], binding)
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
    (output / "summary.json").write_text(json.dumps(result, indent=2) + "\n")
    write_markdown(result, output / "summary.md")
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        with open(summary, "a") as destination:
            destination.write((output / "summary.md").read_text())
    print((output / "summary.md").read_text())


if __name__ == "__main__":
    main()
