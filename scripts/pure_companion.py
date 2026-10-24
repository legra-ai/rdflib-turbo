"""Verify the original pure OWL companion before any Python suite imports it."""

import argparse
import hashlib
import importlib
import json
import re
import tomllib
from email.parser import BytesParser
from pathlib import Path
from zipfile import ZipFile


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def wheel_metadata(wheel):
    with ZipFile(wheel) as archive:
        names = archive.namelist()
        metadata = [name for name in names if name.endswith(".dist-info/METADATA")]
        if len(names) != len(set(names)) or len(metadata) != 1:
            raise ValueError("Pure companion wheel has duplicate or ambiguous metadata")
        prefix = metadata[0].removesuffix("METADATA")
        description = BytesParser().parsebytes(archive.read(metadata[0]))
        layout = BytesParser().parsebytes(archive.read(prefix + "WHEEL"))
        if (
            re.sub(r"[-_.]+", "-", description["Name"].lower()) != "owlrl-turbo"
            or layout["Root-Is-Purelib"] != "true"
            or layout.get_all("Tag") != ["py3-none-any"]
            or any(
                name.endswith((".so", ".pyd", ".dylib", ".dll")) or ".so." in name for name in names
            )
        ):
            raise ValueError("OWL companion must be one universal wheel without native code")
        return description


def wheel_payload(wheel):
    wheel_metadata(wheel)
    with ZipFile(wheel) as archive:
        payload = {
            name: hashlib.sha256(archive.read(name)).hexdigest()
            for name in archive.namelist()
            if name.startswith("owlrl_turbo/") and not name.endswith("/")
        }
    if "owlrl_turbo/__init__.py" not in payload:
        raise ValueError("OWL companion wheel has no package payload")
    return payload


def source_payload(package):
    result = {}
    for path in package.rglob("*"):
        if "__pycache__" in path.parts or path.suffix == ".pyc":
            continue
        if path.is_symlink() or not path.resolve(strict=True).is_relative_to(package.resolve()):
            raise ValueError("OWL companion payload escapes its source package")
        if path.is_file():
            result["owlrl_turbo/" + path.relative_to(package).as_posix()] = digest(path)
    return result


def verify_payload(source, wheel):
    """Bind every package file and project identity to the original source."""
    source, wheel = Path(source), Path(wheel)
    project = tomllib.loads((source / "pyproject.toml").read_text())["project"]
    metadata = wheel_metadata(wheel)
    actual = wheel_payload(wheel)
    if (
        project["name"] != "owlrl-turbo"
        or metadata["Version"] != project["version"]
        or actual != source_payload(source / "src/owlrl_turbo")
    ):
        raise ValueError("Companion wheel payload differs from exact original source")
    return actual


def verify_loaded(wheel, environment):
    """Require the exact original package payload inside the isolated interpreter."""
    wheel = Path(wheel).resolve(strict=True)
    package = importlib.import_module("owlrl_turbo")
    root = Path(package.__file__).resolve(strict=True).parent
    actual = source_payload(root)
    if not root.is_relative_to(Path(environment).resolve()) or actual != wheel_payload(wheel):
        raise ValueError("Installed OWL companion payload differs from its original wheel")
    return {"package": str(root), "payload": actual, "wheel_sha256": digest(wheel)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--wheel", type=Path, required=True)
    parser.add_argument("--environment", type=Path, required=True)
    args = parser.parse_args()
    print(json.dumps(verify_loaded(args.wheel, args.environment)))


if __name__ == "__main__":
    main()
