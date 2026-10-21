"""Test each CPython-specific wheel with its matching isolated interpreter."""

import hashlib
import importlib
import re
import subprocess
import sys
import tempfile
import tomllib
from email.parser import Parser
from pathlib import Path
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[1]


def companion_source():
    """Require the Python consumer and native engine to use one immutable source."""
    project = tomllib.loads((ROOT / "pyproject.toml").read_text())
    cargo = tomllib.loads((ROOT / "Cargo.toml").read_text())
    source = project.get("tool", {}).get("uv", {}).get("sources", {}).get("pyshacl-turbo")
    engine = cargo.get("dependencies", {}).get("pyshacl-turbo-engine")
    if (
        not isinstance(source, dict)
        or set(source) != {"git", "rev"}
        or not isinstance(source["git"], str)
        or not source["git"]
        or not isinstance(source["rev"], str)
        or re.fullmatch(r"[0-9a-fA-F]{40}", source["rev"]) is None
        or not isinstance(engine, dict)
        or any(engine.get(key) != source[key] for key in ("git", "rev"))
    ):
        raise ValueError(
            "Python companion source must match the immutable native engine Git source"
        )
    return source["git"], source["rev"]


def verify_metadata(wheel):
    """Source overrides are test configuration, never public Git requirements."""
    with ZipFile(wheel) as archive:
        members = [name for name in archive.namelist() if name.endswith(".dist-info/METADATA")]
        if len(members) != 1:
            raise ValueError("Expected exactly one wheel METADATA file")
        metadata = Parser().parsestr(archive.read(members[0]).decode())
    if any("git+" in value.lower() for value in metadata.get_all("Requires-Dist", [])):
        raise ValueError("Wheel Requires-Dist must not contain a Git source")


def supported_python_versions():
    # Only the normal driver needs sibling policy; the -I payload verifier does not.
    from release_packages import CPYTHON_VERSIONS

    return CPYTHON_VERSIONS


def select_wheels(directory, version):
    wheels = sorted(directory.glob("*.whl"))
    selected = {}
    for python in supported_python_versions():
        tag = "cp" + python.replace(".", "")
        matches = [
            path for path in wheels if path.name.startswith(f"rdflib_turbo-{version}-{tag}-{tag}-")
        ]
        if len(matches) != 1:
            raise ValueError(f"Expected exactly one CPython {python} wheel")
        selected[python] = matches[0]
    platforms = {path.name.rsplit("-", 1)[1] for path in wheels}
    if len(platforms) != 1 or set(wheels) != set(selected.values()):
        raise ValueError("Expected five matching CPython wheels for one native platform")
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


def test_installed_wheel(wheel, version):
    url, revision = companion_source()
    verify_metadata(wheel)
    major, minor = map(int, version.split("."))
    python_range = f">={version},<{major}.{minor + 1}"
    verify_licenses(wheel)
    with tempfile.TemporaryDirectory() as directory:
        project = Path(directory) / "project"
        commands = [
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
            ["uv", "add", "--project", str(project), "--extra", "test", "--no-sync", str(wheel)],
            [
                "uv",
                "add",
                "--project",
                str(project),
                "--no-sync",
                "--rev",
                revision,
                "git+" + url,
            ],
            ["uv", "sync", "--project", str(project), "--locked"],
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
            ],
            [
                "uv",
                "run",
                "--project",
                str(project),
                "--locked",
                "--no-sync",
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


def test_matrix(wheels):
    for version in supported_python_versions():
        test_installed_wheel(wheels[version], version)


def main():
    if len(sys.argv) > 1:
        if len(sys.argv) != 5 or sys.argv[1] != "--verify-loaded":
            raise ValueError("Expected --verify-loaded WHEEL PYTHON ENVIRONMENT")
        verify_loaded_package(Path(sys.argv[2]), sys.argv[3], Path(sys.argv[4]))
        return
    project = tomllib.loads((ROOT / "pyproject.toml").read_text())
    test_matrix(select_wheels(ROOT / "dist", project["project"]["version"]))


if __name__ == "__main__":
    main()
