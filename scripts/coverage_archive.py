"""Verify a published original source archive without inventing a Git revision."""

import hashlib
import re
import tarfile
import tomllib
from pathlib import PurePosixPath


def digest(source):
    return hashlib.file_digest(source, "sha256").hexdigest()


def member_path(name):
    path = PurePosixPath(name)
    if (
        path.is_absolute()
        or "\\" in name
        or ".." in path.parts
        or len(path.parts) < 2
        or any(part in ("", ".") for part in name.split("/"))
    ):
        raise ValueError(f"Invalid source archive member: {name!r}")
    return path.parts[0], PurePosixPath(*path.parts[1:])


def freeze(archive, source, expected_sha256, *, project="rdflib-turbo"):
    if not re.fullmatch(r"[0-9a-f]{64}", expected_sha256):
        raise ValueError("An explicit original archive SHA-256 is required")
    with archive.open("rb") as stream:
        if digest(stream) != expected_sha256:
            raise ValueError("Original archive SHA-256 differs from the registry receipt")
    root = source.resolve(strict=True)
    # bounded: one hash per file in this fixed package's original source archive.
    files = {}
    prefix = None
    with tarfile.open(archive, "r:gz") as package:
        for member in package:
            if member.isdir():
                continue
            top, relative = member_path(member.name)
            if prefix is not None and top != prefix:
                raise ValueError("Source archive has multiple package roots")
            prefix = top
            if not member.isfile() or str(relative) in files:
                raise ValueError(f"Invalid source archive member: {member.name!r}")
            with package.extractfile(member) as stream:
                expected = digest(stream)
            path = root.joinpath(*relative.parts)
            if (
                not path.resolve(strict=True).is_relative_to(root)
                or path.is_symlink()
                or any(
                    parent.is_symlink() for parent in path.parents if parent.is_relative_to(root)
                )
            ):
                raise ValueError("Extracted source points outside its archive authority")
            with path.open("rb") as stream:
                if digest(stream) != expected:
                    raise ValueError(f"Source bytes differ from original archive: {relative}")
            files[str(relative)] = expected
    if "pyproject.toml" not in files:
        raise ValueError("Source archive has no package manifest")
    manifest = tomllib.loads((root / "pyproject.toml").read_text())
    if project not in {"rdflib-turbo", "pyshacl-turbo", "owlrl-turbo"}:
        raise ValueError("Unsupported source archive project identity")
    if manifest["project"]["name"] != project:
        raise ValueError("Source archive is not the requested native provider or facade")
    return {
        "kind": "published-sdist",
        "filename": archive.name,
        "sha256": expected_sha256,
        "version": manifest["project"]["version"],
        "files": files,
    }
