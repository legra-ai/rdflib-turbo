"""Copy one original source archive to fresh files without restoring stale mtimes."""

import argparse
import shutil
import tarfile
from pathlib import Path, PurePosixPath


def member_parts(member):
    name = member.name.removesuffix("/") if member.isdir() else member.name
    parts = name.split("/")
    if (
        "\\" in name
        or ":" in name
        or any(part in ("", ".", "..") for part in parts)
        or PurePosixPath(name).is_absolute()
        or not (member.isdir() or member.isfile())
        or (len(parts) == 1 and not member.isdir())
    ):
        raise ValueError(f"Invalid source archive member: {member.name!r}")
    return parts


def validated_members(package):
    # bounded: metadata for one fixed source distribution, never RDF input data.
    entries, seen, directories, files = [], set(), set(), set()
    root = None
    for member in package:
        parts = member_parts(member)
        if root is not None and parts[0] != root:
            raise ValueError("Source archive has multiple package roots")
        root = parts[0]
        relative = PurePosixPath(*parts[1:])
        if relative in seen:
            raise ValueError("Source archive has duplicate member paths")
        seen.add(relative)
        if any(parent in files for parent in relative.parents):
            raise ValueError("Source archive member has a regular file as its parent")
        directories.update(relative.parents)
        if member.isfile():
            if relative in directories:
                raise ValueError("Source archive has conflicting file and directory paths")
            files.add(relative)
        else:
            directories.add(relative)
        entries.append((member, relative))
    if not files:
        raise ValueError("Source archive must contain regular source files")
    return entries


def extract(archive, destination):
    """Preserve original content and safe file modes, writing fresh filesystem times."""
    archive, destination = Path(archive), Path(destination)
    if destination.exists() or destination.is_symlink():
        raise FileExistsError(f"Source destination must be fresh: {destination}")
    with tarfile.open(archive, "r:gz") as package:
        entries = validated_members(package)
        destination.mkdir()
        root = destination.resolve(strict=True)
        for member, relative in entries:
            target = destination.joinpath(*relative.parts)
            if not target.resolve().is_relative_to(root):
                raise ValueError("Source archive member escapes its destination")
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            with package.extractfile(member) as source, target.open("xb") as output:
                shutil.copyfileobj(source, output, length=65536)
            target.chmod(member.mode & 0o777)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive-directory", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    args = parser.parse_args()
    archives = list(args.archive_directory.glob("*.tar.gz"))
    if len(archives) != 1 or not archives[0].is_file():
        raise ValueError("Expected exactly one original source archive")
    extract(archives[0], args.destination)


if __name__ == "__main__":
    main()
