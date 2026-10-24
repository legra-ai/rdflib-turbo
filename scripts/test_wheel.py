"""Test each CPython-specific wheel with its matching isolated interpreter."""

import argparse
import hashlib
import importlib
import importlib.util
import re
import subprocess
import sys
import tempfile
import tomllib
from email.parser import Parser
from pathlib import Path
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[1]


def facade_checker():
    """Load only verifier tooling; runtime package imports remain isolated."""
    spec = importlib.util.spec_from_file_location(
        "wheel_facade_checker", Path(__file__).with_name("native_coverage.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def require_facade_wheel(wheel):
    """Source ownership requires an explicit pure facade artifact, never a fallback."""
    if wheel is None or not Path(wheel).is_file():
        raise ValueError("Wheel verification requires an existing pure SHACL facade wheel")
    wheel = Path(wheel).resolve()
    verify_metadata(wheel)
    name = wheel_metadata(wheel).get("Name", "")
    if re.sub(r"[-_.]+", "-", name.lower()) != "pyshacl-turbo":
        raise ValueError("The facade wheel must identify pyshacl-turbo")
    facade_checker().facade_payload(wheel)
    return wheel


def wheel_metadata(wheel):
    """Require one unambiguous distribution metadata record."""
    with ZipFile(wheel) as archive:
        members = [name for name in archive.namelist() if name.endswith(".dist-info/METADATA")]
        if len(members) != 1:
            raise ValueError("Expected exactly one wheel METADATA file")
        return Parser().parsestr(archive.read(members[0]).decode())


def verify_metadata(wheel):
    """Source overrides are test configuration, never public Git requirements."""
    metadata = wheel_metadata(wheel)
    if any("git+" in value.lower() for value in metadata.get_all("Requires-Dist", [])):
        raise ValueError("Wheel Requires-Dist must not contain a Git source")


def supported_python_versions():
    # Only the normal driver needs sibling policy; the -I payload verifier does not.
    from release_packages import CPYTHON_VERSIONS

    return CPYTHON_VERSIONS


def selected_versions(versions=None):
    supported = supported_python_versions()
    if versions is None:
        return supported
    versions = tuple(versions)
    if not versions or versions != tuple(version for version in supported if version in versions):
        raise ValueError("Expected distinct supported CPython versions in policy order")
    return versions


def select_wheels(directory, version, *, versions=None):
    wheels = sorted(directory.glob("*.whl"))
    selected = {}
    for python in selected_versions(versions):
        tag = "cp" + python.replace(".", "")
        matches = [
            path for path in wheels if path.name.startswith(f"rdflib_turbo-{version}-{tag}-{tag}-")
        ]
        if len(matches) != 1:
            raise ValueError(f"Expected exactly one CPython {python} wheel")
        selected[python] = matches[0]
    platforms = {path.name.rsplit("-", 1)[1] for path in wheels}
    if len(platforms) != 1 or set(wheels) != set(selected.values()):
        raise ValueError("Expected matching cohort wheels for one native platform")
    if any(not path.is_file() for path in wheels):
        raise ValueError("Every wheel must be a regular file")
    return selected


def verify_licenses(wheel):
    with ZipFile(wheel) as archive:
        members = archive.namelist()
        for name in ("LICENSE-APACHE", "LICENSE-LEVELDB", "LICENSE-LZ4"):
            relative = f"vendor/oxrocksdb-sys-0.5.11-notices/{name}"
            matches = [member for member in members if member.endswith(f"/licenses/{relative}")]
            if len(matches) != 1:
                raise RuntimeError(f"Wheel is missing bundled native license: {relative}")
            if archive.read(matches[0]) != (ROOT / relative).read_bytes():
                raise RuntimeError(f"Bundled native license changed: {relative}")


def verify_loaded_package(wheel, python, environment):
    if sys.implementation.name != "cpython" or sys.version_info[:2] != tuple(
        map(int, python.split("."))
    ):
        raise RuntimeError(f"Wrong interpreter for wheel: expected CPython {python}")
    package = importlib.import_module("rdflib_turbo")
    native = importlib.import_module("rdflib_turbo._native")
    directory = Path(package.__file__).resolve().parent
    loaded = Path(native.__file__).resolve()
    if not directory.is_relative_to(environment.resolve()) or loaded.parent != directory:
        raise RuntimeError(
            "Loaded wheel package or native extension is outside the isolated environment"
        )
    with ZipFile(wheel) as archive:
        expected = {}
        for name in archive.namelist():
            if name.startswith("rdflib_turbo/") and name.endswith((".py", ".so", ".pyd")):
                with archive.open(name) as source:
                    expected[name] = hashlib.file_digest(source, "sha256").hexdigest()
    actual = {}
    for path in directory.rglob("*"):
        if path.is_file() and path.suffix in (".py", ".so", ".pyd"):
            with path.open("rb") as source:
                actual["rdflib_turbo/" + path.relative_to(directory).as_posix()] = (
                    hashlib.file_digest(source, "sha256").hexdigest()
                )
    if actual != expected:
        raise RuntimeError("Installed wheel payload differs from the tested archive")
    native_members = [name for name in expected if name.endswith((".so", ".pyd"))]
    if native_members != ["rdflib_turbo/" + loaded.name]:
        raise RuntimeError("Loaded native extension differs from the wheel payload")
    print(f"Verified CPython {python} and every installed wheel source/native hash", flush=True)


def test_installed_wheel(wheel, version, shacl_wheel):
    shacl_wheel = require_facade_wheel(shacl_wheel)
    wheel = Path(wheel).resolve()
    verify_metadata(wheel)
    major, minor = map(int, version.split("."))
    python_range = f">={version},<{major}.{minor + 1}"
    verify_licenses(wheel)
    with tempfile.TemporaryDirectory() as directory:
        project = Path(directory) / "project"
        dependencies = Path(directory) / "requirements.lock"
        commands = [
            [
                "uv",
                "export",
                "--project",
                str(ROOT),
                "--locked",
                "--group",
                "coverage",
                "--no-dev",
                "--no-emit-project",
                "--no-emit-package",
                "pyshacl-turbo",
                "--no-emit-package",
                "owlrl-turbo",
                "--no-hashes",
                "--output-file",
                str(dependencies),
            ],
            [
                "uv",
                "init",
                "--bare",
                "--no-workspace",
                "--vcs",
                "none",
                "--python",
                python_range,
                str(project),
            ],
            ["uv", "python", "pin", "--project", str(project), version],
            [
                "uv",
                "add",
                "--project",
                str(project),
                "--no-sync",
                "--requirements",
                str(dependencies),
            ],
            ["uv", "add", "--project", str(project), "--extra", "test", "--no-sync", str(wheel)],
            [
                "uv",
                "add",
                "--project",
                str(project),
                "--no-sync",
                str(shacl_wheel),
            ],
            [
                "uv",
                "sync",
                "--project",
                str(project),
                "--locked",
                "--no-build",
            ],
            [
                "uv",
                "run",
                "--project",
                str(project),
                "--locked",
                "--no-sync",
                "python",
                "-I",
                str(Path(__file__).resolve()),
                "--verify-loaded",
                str(wheel),
                version,
                str(project / ".venv"),
                str(shacl_wheel),
            ],
            [
                "uv",
                "run",
                "--project",
                str(project),
                "--locked",
                "--no-sync",
                "python",
                "-I",
                "-m",
                "pytest",
                str(ROOT / "tests"),
                "-q",
                "--import-mode=importlib",
                "--cov=rdflib_turbo",
                "--cov-branch",
                "--cov-fail-under=100",
            ],
        ]
        for command in commands:
            subprocess.run(command, cwd=directory, check=True)


def test_matrix(wheels, shacl_wheel, *, versions=None):
    versions = selected_versions(versions)
    if set(wheels) != set(versions):
        raise ValueError("Wheel inventory must exactly match the requested interpreter cohort")
    for version in versions:
        test_installed_wheel(wheels[version], version, shacl_wheel)


def main():
    if len(sys.argv) > 1 and sys.argv[1] == "--verify-loaded":
        if len(sys.argv) != 6:
            raise ValueError("Expected --verify-loaded WHEEL PYTHON ENVIRONMENT SHACL_WHEEL")
        verify_loaded_package(Path(sys.argv[2]), sys.argv[3], Path(sys.argv[4]))
        shacl_wheel = require_facade_wheel(Path(sys.argv[5]))
        facade_checker().verify_loaded_facade(shacl_wheel, Path(sys.argv[4]))
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--shacl-wheel", type=Path, required=True)
    parser.add_argument("--python-versions", nargs="+")
    args = parser.parse_args()
    shacl_wheel = require_facade_wheel(args.shacl_wheel)
    project = tomllib.loads((ROOT / "pyproject.toml").read_text())
    versions = selected_versions(args.python_versions)
    wheels = select_wheels(ROOT / "dist", project["project"]["version"], versions=versions)
    test_matrix(wheels, shacl_wheel, versions=versions)


if __name__ == "__main__":
    main()
