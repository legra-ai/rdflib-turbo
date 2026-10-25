"""Select an original lock-declaration peer independently of current acceptance."""

import importlib.util
import re
import tomllib
from pathlib import Path


def archive_authority(source, archive, sha256):
    spec = importlib.util.spec_from_file_location(
        "coverage_archive", Path(__file__).with_name("coverage_archive.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    # Explicit archive input proves original bytes; registry availability is separate.
    return {
        **module.freeze(Path(archive), source, sha256, project="pyshacl-turbo"),
        "kind": "original-sdist",
    }


def locked_version(owner):
    lock = tomllib.loads((owner / "uv.lock").read_text())
    packages = [package for package in lock["package"] if package["name"] == "pyshacl-turbo"]
    if len(packages) != 1:
        raise ValueError("Expected exactly one original facade declaration peer in the owner lock")
    return packages[0]["version"]


def select(
    owner, facade, facade_input, freeze, *, source=None, revision=None, archive=None, sha256=None
):
    explicit = any(value is not None for value in (source, revision, archive, sha256))
    if explicit:
        git_mode = (
            source is not None and revision is not None and archive is None and sha256 is None
        )
        archive_mode = (
            source is not None and revision is None and archive is not None and sha256 is not None
        )
        if not (git_mode or archive_mode):
            raise ValueError("Choose exactly one complete declaration authority")
        source = Path(source).resolve(strict=True)
        if git_mode:
            if not re.fullmatch(r"[0-9a-f]{40}", revision):
                raise ValueError("Declaration Git authority requires an immutable revision")
            authority = {"kind": "git", **freeze(source, revision)}
        else:
            authority = archive_authority(source, archive, sha256)
    else:
        source = facade.resolve(strict=True)
        authority = {"kind": "git", **facade_input}
    manifest = tomllib.loads((source / "pyproject.toml").read_text())
    if manifest["project"]["name"] != "pyshacl-turbo" or manifest["project"][
        "version"
    ] != locked_version(owner):
        raise ValueError(
            "An independently frozen declaration peer must match the original owner lock"
        )
    return {"source": source, "authority": authority}


def verify_receipt(record, archive, archive_reader):
    if not isinstance(record, dict) or record.get("authority", {}).get("kind") not in {
        "git",
        "original-sdist",
    }:
        raise ValueError("Candidate receipt requires original declaration peer authority")
    authority = record["authority"]
    files, metadata = archive_reader(archive, record["archive_sha256"])
    manifest = tomllib.loads(metadata["pyproject.toml"].decode())
    if (
        manifest["project"]["name"] != "pyshacl-turbo"
        or manifest["project"]["version"] != record["version"]
    ):
        raise ValueError("Declaration archive differs from its original locked version")
    if authority["kind"] == "git":
        if not re.fullmatch(r"[0-9a-f]{40}", authority.get("revision", "")) or any(
            authority["files"].get(name) != sha for name, sha in files.items() if name != "PKG-INFO"
        ):
            raise ValueError("Declaration archive differs from its immutable Git source")
    elif (
        "revision" in authority
        or authority["sha256"] != record["archive_sha256"]
        or authority["files"] != files
    ):
        raise ValueError("Declaration archive differs from its original source authority")
    return record
