"""Reject private development documents in the source archive before publication."""

import sys
import tarfile
from pathlib import PurePosixPath


def audit(path):
    with tarfile.open(path, "r:gz") as archive:
        for entry in archive:
            member = PurePosixPath(entry.name)
            if member.is_absolute() or ".." in member.parts:
                raise ValueError(f"Unsafe archive path: {entry.name}")
            if member.parts[1:3] in (("docs", "plans"), ("docs", "parity-evidence")):
                raise ValueError(f"Private document in source archive: {entry.name}")


def main():
    if len(sys.argv) != 2:
        raise ValueError("Expected one source archive")
    audit(sys.argv[1])


if __name__ == "__main__":
    main()
