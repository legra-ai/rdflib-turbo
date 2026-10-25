"""Reconstruct an explicit lock declaration from raw Git and its original uv sdist."""

import hashlib
import importlib.util
import os
import subprocess
import tomllib
from pathlib import Path, PurePosixPath


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def selected(owner):
    return load("ci_facade").policy(Path(owner) / "scripts/ci-facade.json")["declaration"]


def git(repository, *arguments):
    return subprocess.check_output(["git", "-C", str(repository), *arguments])


def digest(path):
    with Path(path).open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def raw_tree(repository, revision, destination=None):
    """Read blobs without checkout filters, newline conversion or executable loss."""
    files, modes = {}, {}  # bounded: one fixed source repository's tracked inventory.
    if destination is not None:
        destination.mkdir()
    for entry in git(repository, "ls-tree", "-r", "-z", "--full-tree", revision).split(b"\0"):
        if not entry:
            continue
        metadata, raw_name = entry.split(b"\t", 1)
        mode, kind, blob = metadata.decode().split()
        name = raw_name.decode("utf-8")
        parts = name.split("/")
        if (
            mode not in {"100644", "100755"}
            or kind != "blob"
            or PurePosixPath(name).is_absolute()
            or "\\" in name
            or ":" in name
            or any(part in {"", ".", "..", ".git"} for part in parts)
        ):
            raise ValueError("Declaration requires safe regular Git blob paths")
        content = git(repository, "cat-file", "blob", blob)
        files[name] = hashlib.sha256(content).hexdigest()
        modes[name] = mode
        if destination is not None:
            target = destination.joinpath(*parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            with target.open("xb") as stream:
                stream.write(content)
            target.chmod(0o755 if mode == "100755" else 0o644)
    if not files:
        raise ValueError("Declaration Git authority has no source files")
    return files, modes


def inventory(source):
    files = {}  # bounded: this exact original source distribution, never RDF data.
    for path in Path(source).rglob("*"):
        if path.is_symlink() or not (path.is_file() or path.is_dir()):
            raise ValueError("Declaration inventory contains a non-regular source path")
        if path.is_file():
            files[path.relative_to(source).as_posix()] = digest(path)
    return files


def identity(owner, source):
    manifest = tomllib.loads((source / "pyproject.toml").read_text())
    version = load("facade_declaration").locked_version(Path(owner))
    if (
        manifest["project"]["name"] != "pyshacl-turbo"
        or manifest["project"]["version"] != version
        or manifest["build-system"]["build-backend"] != "hatchling.build"
    ):
        raise ValueError("Declaration must be a pure facade matching the owner's locked version")
    return version


def build(source, output, python):
    output.mkdir()
    subprocess.run(
        ["uv", "build", "--sdist", "--python", str(python), "--out-dir", str(output), str(source)],
        check=True,
    )
    archives = list(output.iterdir())
    if len(archives) != 1 or not archives[0].name.endswith(".tar.gz"):
        raise ValueError("Declaration build must produce one original source archive")
    return archives[0]


def prepare(owner, output, python):
    owner, output = Path(owner).resolve(strict=True), Path(output).resolve()
    authority = selected(owner)
    if output.is_relative_to(owner):
        raise ValueError("Declaration reconstruction must stay outside the frozen owner")
    output.mkdir(parents=True, exist_ok=False)
    repository = output / "authority.git"
    subprocess.run(["git", "init", "--bare", str(repository)], check=True)
    subprocess.run(
        ["git", "-C", str(repository), "remote", "add", "origin", authority["repository"]],
        check=True,
    )
    subprocess.run(
        ["git", "-C", str(repository), "fetch", "--depth=1", "origin", authority["revision"]],
        check=True,
    )
    source = output / "raw-source"
    files, modes = raw_tree(repository, authority["revision"], source)
    version = identity(owner, source)
    archive = build(source, output / "originals", python)
    if inventory(source) != files:
        raise ValueError("Raw declaration source inventory changed during original archive build")
    extracted = output / "source"
    load("extract_source").extract(archive, extracted)
    proof = load("coverage_archive").freeze(
        archive, extracted, digest(archive), project="pyshacl-turbo"
    )
    record = {
        "role": "uv-export-declaration",
        "project": "pyshacl-turbo",
        "version": version,
        "source": str(extracted),
        "authority": {"kind": "git", **authority, "files": files, "modes": modes},
        "archive": str(archive),
        "archive_sha256": proof["sha256"],
        "archive_files": proof["files"],
        "build": {
            "repository": str(repository),
            "source": str(source),
            "python": str(python),
            "command": "uv build --sdist",
        },
    }
    return verify(owner, record)


def verify(owner, record, initial=True):
    """Verify unchanged original bytes; declaration sources never permit generated files."""
    owner = Path(owner).resolve(strict=True)
    policy = selected(owner)
    authority = record.get("authority", {})
    if (
        record.get("role") != "uv-export-declaration"
        or record.get("project") != "pyshacl-turbo"
        or authority.get("kind") != "git"
        or any(authority.get(key) != value for key, value in policy.items())
    ):
        raise ValueError("Declaration authority differs from explicit owner policy")
    repository = Path(record["build"]["repository"])
    if git(repository, "remote", "get-url", "origin").decode().strip() != policy["repository"]:
        raise ValueError("Declaration raw Git authority repository changed")
    files, modes = raw_tree(repository, policy["revision"])
    if files != authority["files"] or modes != authority["modes"]:
        raise ValueError("Declaration raw Git authority inventory changed")
    source, archive = Path(record["source"]), Path(record["archive"])
    if identity(owner, source) != record["version"]:
        raise ValueError("Declaration locked version changed")
    proof = load("coverage_archive").freeze(
        archive, source, record["archive_sha256"], project="pyshacl-turbo"
    )
    if proof["files"] != record["archive_files"] or inventory(source) != record["archive_files"]:
        raise ValueError("Declaration original archive inventory changed")
    if any(files.get(name) != sha for name, sha in proof["files"].items() if name != "PKG-INFO"):
        raise ValueError("Original declaration archive differs from raw Git authority")
    if os.name != "nt" and any(
        bool((source / name).stat().st_mode & 0o111) != (modes[name] == "100755")
        for name in proof["files"]
        if name != "PKG-INFO"
    ):
        raise ValueError("Declaration original executable modes differ from Git authority")
    return record
