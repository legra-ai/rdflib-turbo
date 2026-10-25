"""Original-lock pure PyYAML test inputs; never a publishable dependency release."""

import argparse
import email.parser
import hashlib
import importlib
import json
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tomllib
import urllib.request
from pathlib import Path, PurePosixPath, PureWindowsPath
from zipfile import ZipFile

VERSION = "6.0.3"
PYTHON = "3.13.16"
DSO = (".so", ".pyd", ".dll", ".dylib")
KIND = "test-only-yaml-dependency"


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def _sha(data):
    return hashlib.sha256(data).hexdigest()


def _locked(owner):
    path = Path(owner) / "uv.lock"
    entries = [p for p in tomllib.loads(path.read_text())["package"] if p["name"] == "pyyaml"]
    if len(entries) != 1 or entries[0].get("version") != VERSION:
        raise ValueError("Expected one original locked PyYAML 6.0.3 entry")
    entry = entries[0]
    sdist = entry.get("sdist", {})
    if not sdist.get("url", "").startswith("https://files.pythonhosted.org/") or not re.fullmatch(
        r"sha256:[0-9a-f]{64}", sdist.get("hash", "")
    ):
        raise ValueError("Expected the original lock's exact PyYAML sdist URL/SHA")
    return entry, digest(path)


def _relative(name):
    parts = name.split("/")
    if "\\" in name or ":" in name or any(p in ("", ".", "..") for p in parts):
        raise ValueError("Unsafe dependency path")
    value = PurePosixPath(name)
    if value.is_absolute():
        raise ValueError("Absolute dependency path")
    return value


def _path(root, name):
    path = root.joinpath(*_relative(name).parts)
    if path.is_symlink() or not path.resolve().is_relative_to(root.resolve()):
        raise ValueError("Dependency path escapes its authority")
    return path


def _archive(archive):
    # bounded: the one checksum-pinned test dependency source archive.
    files, seen, prefix = {}, set(), None
    with tarfile.open(archive, "r:gz") as stream:
        for member in stream:
            name = member.name.removesuffix("/") if member.isdir() else member.name
            parts = _relative(name).parts
            if prefix is None:
                prefix = parts[0]
            if parts[0] != prefix or name in seen or not (member.isdir() or member.isfile()):
                raise ValueError("Invalid original dependency archive member")
            seen.add(name)
            if member.isdir():
                continue
            if len(parts) < 2:
                raise ValueError("Original dependency archive requires one source root")
            relative = PurePosixPath(*parts[1:]).as_posix()
            if any(parent.as_posix() in files for parent in PurePosixPath(relative).parents):
                raise ValueError("Original archive file is also a directory")
            files[relative] = (stream.extractfile(member).read(), member.mode & 0o777)
    if not files:
        raise ValueError("Empty original dependency archive")
    for name in files:
        if any(parent.as_posix() in files for parent in PurePosixPath(name).parents):
            raise ValueError("Original archive has conflicting file/directory entries")
    return files


def _source_payload(files):
    payload = {
        name.removeprefix("lib/"): _sha(data)
        for name, (data, _) in files.items()
        if name.startswith(("lib/yaml/", "lib/_yaml/"))
    }
    if not {"yaml/__init__.py", "_yaml/__init__.py"} <= payload.keys():
        raise ValueError("Original archive lacks both complete YAML packages")
    return payload


def _wheel(wheel):
    with ZipFile(wheel) as stream:
        names = stream.namelist()
        if len(names) != len(set(names)):
            raise ValueError("Duplicate wheel members")
        for name in names:
            _relative(name.removesuffix("/"))
            if stream.getinfo(name).external_attr >> 16 & 0o170000 == 0o120000:
                raise ValueError("Wheel symlink")
        metadata = [name for name in names if name.endswith(".dist-info/METADATA")]
        wheel_info = [name for name in names if name.endswith(".dist-info/WHEEL")]
        if (
            len(metadata) != 1
            or len(wheel_info) != 1
            or metadata[0].rsplit("/", 1)[0] != wheel_info[0].rsplit("/", 1)[0]
        ):
            raise ValueError("Expected one canonical wheel metadata authority")
        info = email.parser.BytesParser().parsebytes(stream.read(metadata[0]))
        tags = email.parser.BytesParser().parsebytes(stream.read(wheel_info[0]))
        if (
            len(info.get_all("Name", [])) != 1
            or len(info.get_all("Version", [])) != 1
            or re.sub(r"[-_.]+", "-", info.get("Name", "")).lower() != "pyyaml"
            or info.get("Version") != VERSION
        ):
            raise ValueError("Wrong YAML wheel name/version")
        fields = Path(wheel).name.removesuffix(".whl").split("-")
        if (
            len(fields) != 5
            or re.sub(r"[-_.]+", "-", fields[0]).lower() != "pyyaml"
            or fields[1] != VERSION
        ):
            raise ValueError("Wheel filename differs from YAML metadata")
        if (
            len(tags.get_all("Root-Is-Purelib", [])) != 1
            or tags.get("Root-Is-Purelib", "").lower() != "true"
            or tags.get_all("Tag") != ["py3-none-any"]
        ):
            raise ValueError("YAML test wheel must be universal pure Python")
        if not Path(wheel).name.endswith("-py3-none-any.whl") or any(
            name.lower().endswith(DSO) for name in names
        ):
            raise ValueError("YAML test wheel has a native payload or tag")
        payload = {
            name: _sha(stream.read(name))
            for name in names
            if name.startswith(("yaml/", "_yaml/")) and not name.endswith("/")
        }
        if any(
            not name.startswith(("yaml/", "_yaml/", metadata[0].split("/")[0] + "/"))
            for name in names
        ):
            raise ValueError("Unexpected runtime namespace in YAML wheel")
        return payload


def _reference(source):
    result = {}
    for path in source.rglob("*"):
        if path.is_symlink():
            raise ValueError("Reference source contains a symlink")
        if path.is_file():
            result[path.relative_to(source).as_posix()] = digest(path)
        elif not path.is_dir():
            raise ValueError("Reference source contains a nonregular entry")
    return result


def _download(url, destination):
    with urllib.request.urlopen(url) as response, destination.open("xb") as output:
        shutil.copyfileobj(response, output)


def _run(command, directory, environment):
    return subprocess.check_output(
        command, cwd=directory, env=environment, text=True, stderr=subprocess.STDOUT
    )


def _command(archive, python):
    return [
        "uv",
        "build",
        "--wheel",
        "--no-sources",
        "--python",
        python,
        "--out-dir",
        "wheels",
        archive,
    ]


def _python_command(python):
    return [
        "uv",
        "run",
        "--no-project",
        "--python",
        python,
        "python",
        "-I",
        "-B",
        "-c",
        "import platform; print(platform.python_version())",
    ]


def prepare(owner, output, python):
    """Build one explicit test-only pure wheel from the owner's original locked sdist."""
    entry, lock_sha = _locked(owner)
    python = str(python)
    if python != PYTHON:
        raise ValueError("YAML test inputs require Python 3.13.16")
    output = Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    (output / "originals").mkdir()
    name = entry["sdist"]["url"].rsplit("/", 1)[-1]
    _relative(name)
    archive_name = "originals/" + name
    archive = output / archive_name
    _download(entry["sdist"]["url"], archive)
    if digest(archive) != entry["sdist"]["hash"].removeprefix("sha256:"):
        raise ValueError("Original locked PyYAML archive SHA mismatch")
    files = _archive(archive)
    payload = _source_payload(files)
    source = output / "source"
    source.mkdir()
    for name, (data, mode) in files.items():
        path = source.joinpath(*PurePosixPath(name).parts)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        path.chmod(mode)
    archive_files = {name: _sha(data) for name, (data, _) in files.items()}
    overrides = {
        "PYYAML_FORCE_LIBYAML": "0",
        "TMPDIR": "build-scratch",
        "TMP": "build-scratch",
        "TEMP": "build-scratch",
        "UV_CACHE_DIR": "build-cache",
    }
    environment = {
        k: v
        for k, v in os.environ.items()
        if k
        not in {"VIRTUAL_ENV", "PYTHONPATH", "PYTHONHOME", "UV_PROJECT", "UV_PROJECT_ENVIRONMENT"}
    }
    for key, value in overrides.items():
        environment[key] = value if key == "PYYAML_FORCE_LIBYAML" else str(output / value)
    (output / "build-scratch").mkdir()
    uv = _run(["uv", "--version"], output, environment).strip()
    if not uv.startswith("uv 0.13.1 "):
        raise ValueError("YAML inputs require pinned uv 0.13.1")
    python_command = _python_command(python)
    actual_python = _run(python_command, output, environment).strip()
    if actual_python != PYTHON:
        raise ValueError("Actual YAML build interpreter differs from Python 3.13.16")
    command = _command(archive_name, python)
    (output / "build.log").write_text(_run(command, output, environment))
    wheels = list((output / "wheels").glob("*.whl"))
    if len(wheels) != 1:
        raise ValueError("Expected one original pure YAML test wheel")
    wheel = wheels[0]
    if _wheel(wheel) != payload or _reference(source) != archive_files:
        raise ValueError("Build changed original YAML source/runtime payload")
    record = {
        "schema": 1,
        "kind": KIND,
        "publishable": False,
        "owner_lock_sha256": lock_sha,
        "locked_entry": entry,
        "archive": archive_name,
        "archive_sha256": digest(archive),
        "source": "source",
        "archive_files": archive_files,
        "wheel": wheel.relative_to(output).as_posix(),
        "wheel_sha256": digest(wheel),
        "package_payload": payload,
        "recipe": {
            "command": command,
            "python_command": python_command,
            "cwd": ".",
            "environment": overrides,
        },
        "provenance": {
            "uv": uv,
            "python": actual_python,
            "command": command,
            "python_command": python_command,
            "cwd": str(output),
            "environment": {key: environment[key] for key in overrides},
            "executable": shutil.which("uv", path=environment.get("PATH")),
        },
    }
    authority = output / "yaml-inputs.json"
    authority.write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    verify(owner, authority)
    return record


def verify(owner, authority):
    """Verify an explicit portable authority and its untouched reference tree, read-only."""
    authority = Path(authority)
    if not authority.is_file() or authority.is_symlink():
        raise ValueError("Expected an explicit YAML authority JSON file")
    record = json.loads(authority.read_text())
    root = authority.resolve().parent
    entry, lock_sha = _locked(owner)
    if (
        type(record.get("schema")) is not int
        or record.get("schema") != 1
        or record.get("kind") != KIND
        or record.get("publishable") is not False
    ):
        raise ValueError("Wrong YAML test-only authority")
    if record.get("owner_lock_sha256") != lock_sha or record.get("locked_entry") != entry:
        raise ValueError("YAML authority differs from the original owner lock")
    archive, wheel, source = (_path(root, record[name]) for name in ("archive", "wheel", "source"))
    if digest(archive) != record["archive_sha256"] or digest(archive) != entry["sdist"][
        "hash"
    ].removeprefix("sha256:"):
        raise ValueError("Original YAML archive changed")
    files = _archive(archive)
    expected = {name: _sha(data) for name, (data, _) in files.items()}
    if expected != record["archive_files"] or _reference(source) != expected:
        raise ValueError("Original YAML reference source changed")
    if (
        digest(wheel) != record["wheel_sha256"]
        or _wheel(wheel) != _source_payload(files)
        or _wheel(wheel) != record["package_payload"]
    ):
        raise ValueError("Original YAML wheel payload changed")
    overrides = {
        "PYYAML_FORCE_LIBYAML": "0",
        "TMPDIR": "build-scratch",
        "TMP": "build-scratch",
        "TEMP": "build-scratch",
        "UV_CACHE_DIR": "build-cache",
    }
    recipe = {
        "command": _command(record["archive"], PYTHON),
        "python_command": _python_command(PYTHON),
        "cwd": ".",
        "environment": overrides,
    }
    provenance = record["provenance"]
    historical = provenance.get("cwd", "")
    historical = (
        PureWindowsPath(historical)
        if PureWindowsPath(historical).is_absolute()
        else PurePosixPath(historical)
    )
    expected_environment = {
        key: value if key == "PYYAML_FORCE_LIBYAML" else str(historical / value)
        for key, value in overrides.items()
    }
    # Historical paths are compared as strings only; a relocated consumer must
    # never require or execute the producer's interpreter, scratch or directory.
    if (
        record.get("recipe") != recipe
        or not historical.is_absolute()
        or provenance.get("python") != PYTHON
        or not provenance.get("uv", "").startswith("uv 0.13.1 ")
        or provenance.get("command") != recipe["command"]
        or provenance.get("python_command") != recipe["python_command"]
        or provenance.get("environment") != expected_environment
        or not provenance.get("executable")
    ):
        raise ValueError("YAML build provenance changed")
    return {"record": record, "source": source, "wheel": wheel, "archive": archive}


def verify_loaded(wheel, venv):
    """Verify complete installed bytes and supported loaded modules without C-module probes."""
    expected = _wheel(wheel)
    module = importlib.import_module("yaml")
    if (
        getattr(module, "__version__", None) != VERSION
        or getattr(module, "__with_libyaml__", False) is not False
    ):
        raise ValueError("Loaded YAML version/purity differs from its original pure wheel")
    origin = Path(module.__file__).resolve().parent
    if not origin.is_relative_to(Path(venv).resolve()):
        raise ValueError("YAML imported outside its actual isolated environment")
    base = origin.parent
    actual = {}
    for package in ("yaml", "_yaml"):
        for path in (base / package).rglob("*"):
            if path.is_symlink():
                raise ValueError("Installed YAML symlink")
            if path.is_file() and "__pycache__" not in path.parts:
                actual[package + "/" + path.relative_to(base / package).as_posix()] = digest(path)
    if actual != expected or any(path.name.lower().endswith(DSO) for path in base.glob("_yaml*")):
        raise ValueError("Installed YAML payload differs from the original pure wheel")
    loaded = {}
    for name, loaded_module in tuple(sys.modules.items()):
        if loaded_module is None or not (
            name in ("yaml", "_yaml") or name.startswith(("yaml.", "_yaml."))
        ):
            continue
        path = Path(loaded_module.__file__).resolve()
        if not path.is_relative_to(base):
            raise ValueError("Loaded YAML module has an external origin")
        relative = path.relative_to(base).as_posix()
        if relative not in expected or digest(path) != expected[relative]:
            raise ValueError("Loaded YAML module differs from its original wheel")
        loaded[name] = relative
    return {
        "wheel_sha256": digest(wheel),
        "origin": str(origin),
        "payload": actual,
        "loaded_modules": loaded,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel", type=Path, required=True)
    parser.add_argument("--environment", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(verify_loaded(args.wheel, args.environment), sort_keys=True))


if __name__ == "__main__":
    main()
