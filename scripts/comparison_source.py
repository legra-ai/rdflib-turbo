"""Authenticate original comparison source trees without package imports."""

import copy
import hashlib
import subprocess
import tarfile
import tomllib
from pathlib import PurePosixPath


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def freeze(source):
    dirty = subprocess.check_output(["git", "-C", str(source), "status", "--porcelain"])
    if dirty:
        raise ValueError("Source checkout is not clean")
    names = (
        subprocess.check_output(["git", "-C", str(source), "ls-files", "-z"]).decode().split("\0")
    )
    return {name: sha(source / name) for name in names if name}


def normalized_manifest(original, archived):
    expected = copy.deepcopy(original)
    if "readme" not in expected["package"] and archived["package"].get("readme") == "README.md":
        expected["package"]["readme"] = "README.md"
    if expected == archived:
        return
    workspace = expected.get("workspace", {})
    members = workspace.get("members")
    if members is not None:
        # Maturin keeps the root package implicit in the packaged workspace.
        # A distinct default selection is authoritative and must remain exact.
        if workspace.get("default-members") == members:
            del workspace["default-members"]
        workspace["members"] = [member for member in members if member != "."]
    if expected != archived:
        raise ValueError("Unexpected Cargo manifest normalization")


def audit_tree(source, extracted, names):
    included = {}
    for name in names:
        path = extracted / name
        required = name.startswith(
            (
                "src/",
                "python/",
                "vendor/",
                "crates/",
                "build_support/",
                "tests/",
                "scripts/",
                ".github/",
            )
        )
        required |= name in (
            "Cargo.toml",
            "Cargo.lock",
            "pyproject.toml",
            "uv.lock",
            "rust-toolchain.toml",
            ".cargo/config.toml",
        )
        if not path.is_file():
            if required:
                raise ValueError(f"Missing source archive input: {name}")
            continue
        if name == "Cargo.toml":
            normalized_manifest(
                tomllib.loads((source / name).read_text()), tomllib.loads(path.read_text())
            )
        elif sha(path) != sha(source / name):
            raise ValueError(f"Changed source archive input: {name}")
        included[name] = sha(path)
    return included


def archive_files(archive, prefix):
    """Return the complete regular payload of one original source archive."""
    result = {}
    with tarfile.open(archive) as bundle:
        seen = set()
        for entry in bundle:
            name = PurePosixPath(entry.name)
            if (
                name.is_absolute()
                or ".." in name.parts
                or "\\" in entry.name
                or not name.parts
                or name.parts[0] != prefix
                or entry.name in seen
            ):
                raise ValueError("Unsafe or duplicate original source archive member")
            seen.add(entry.name)
            if entry.isdir():
                continue
            if not entry.isfile() or len(name.parts) < 2:
                raise ValueError("Original source archive contains a link or special member")
            relative = PurePosixPath(*name.parts[1:]).as_posix()
            if relative in result:
                raise ValueError("Duplicate original source archive payload")
            result[relative] = hashlib.sha256(bundle.extractfile(entry).read()).hexdigest()
    if not result:
        raise ValueError("Empty original source archive")
    return result


def tree_files(root, *, cargo_marker=False):
    if root.is_symlink() or not root.is_dir():
        raise ValueError("Source root must be an actual directory")
    result = {}
    for path in root.rglob("*"):
        if path.is_symlink():
            raise ValueError("Source payload contains a symbolic link")
        if path.is_dir():
            continue
        if not path.is_file():
            raise ValueError("Source payload contains a special file")
        name = path.relative_to(root).as_posix()
        if cargo_marker and name == ".cargo-ok":
            continue
        result[name] = sha(path)
    return result


def verify_tree(root, archive, checksum, *, cargo_marker=False):
    if sha(archive) != checksum:
        raise ValueError("Original source archive checksum mismatch")
    expected = archive_files(archive, root.name)
    if tree_files(root, cargo_marker=cargo_marker) != expected:
        raise ValueError("Compiled source differs from its complete original archive")
    return expected
