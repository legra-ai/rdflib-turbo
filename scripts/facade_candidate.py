"""Validate test-only facade metadata without altering its program bytes."""

import copy
import email
import hashlib
import importlib.util
import json
import re
import tarfile
import tomllib
import zipfile
from pathlib import Path, PurePosixPath


def candidate_version(version, facade_revision, provider_revision):
    if not all(
        re.fullmatch(r"[0-9a-f]{40}", value) for value in (facade_revision, provider_revision)
    ):
        raise ValueError("Candidate binding requires both immutable source revisions")
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:(?:a|b|rc)\d+)?", version):
        raise ValueError("Candidate input must have a canonical release version")
    return f"{version}+candidate.{facade_revision[:12]}.{provider_revision[:12]}"


def is_provider(value):
    return re.match(r"rdflib[-_]turbo\b", value, re.IGNORECASE) is not None


def bind(values, version, *, required):
    matches = [index for index, value in enumerate(values) if is_provider(value)]
    if len(matches) > 1 or (required and len(matches) != 1):
        raise ValueError("Expected one prepared native provider binding")
    result = list(values)
    for index in matches:
        result[index] = f"rdflib-turbo=={version}"
    return result


def bound_manifest(original, provider_version, version, binding):
    if original["project"]["name"] != "pyshacl-turbo":
        raise ValueError("Candidate source must be the real SHACL facade")
    if original["build-system"]["build-backend"] != "hatchling.build":
        raise ValueError("Candidate source must already build a pure facade")
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:(?:a|b|rc)\d+)?", provider_version):
        raise ValueError("Expected the actual prepared provider version")
    if "+candidate." not in version:
        raise ValueError("Candidate metadata requires its nonpublishable namespace")
    result = copy.deepcopy(original)
    project = result["project"]
    project["version"] = version
    project["dependencies"] = bind(project["dependencies"], provider_version, required=True)
    optional = project.get("optional-dependencies", {})
    if "test" in optional:
        optional["test"] = bind(optional["test"], provider_version, required=False)
    result.setdefault("tool", {}).setdefault("uv", {}).setdefault("sources", {})["rdflib-turbo"] = {
        "path": binding,
        "editable": True,
    }
    return result


def verify_manifest(original, candidate, provider_version, version, binding):
    if candidate != bound_manifest(original, provider_version, version, binding):
        raise ValueError("Candidate source must contain only the dependency-only metadata binding")


def verify_lock(original, candidate):
    if {key: value for key, value in original.items() if key != "package"} != {
        key: value for key, value in candidate.items() if key != "package"
    }:
        raise ValueError("Candidate changed unrelated locked dependencies or resolution settings")
    names = {"pyshacl-turbo", "rdflib-turbo"}
    before = [value for value in original["package"] if value["name"] not in names]
    after = [value for value in candidate["package"] if value["name"] not in names]
    if before != after:
        raise ValueError("Candidate changed unrelated locked dependencies")


def archive_record(path, expected_sha256):
    with path.open("rb") as stream:
        if hashlib.file_digest(stream, "sha256").hexdigest() != expected_sha256:
            raise ValueError("Original candidate input archive SHA-256 differs")
    spec = importlib.util.spec_from_file_location(
        "coverage_archive", Path(__file__).with_name("coverage_archive.py")
    )
    rules = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(rules)
    files, metadata, prefix = {}, {}, None
    with tarfile.open(path, "r:gz") as archive:
        for member in archive:
            if member.isdir():
                continue
            top, relative = rules.member_path(member.name)
            name = str(relative)
            if not member.isfile() or name in files or (prefix is not None and prefix != top):
                raise ValueError("Invalid candidate source archive member")
            prefix = top
            with archive.extractfile(member) as stream:
                files[name] = hashlib.file_digest(stream, "sha256").hexdigest()
            if name in {"pyproject.toml", "uv.lock", "PKG-INFO"}:
                # bounded: three fixed package metadata files, never program payloads.
                metadata[name] = archive.extractfile(member).read()
    if set(metadata) != {"pyproject.toml", "uv.lock", "PKG-INFO"}:
        raise ValueError("Candidate source archive is missing its package metadata")
    return files, metadata


def verify_pkginfo(original, candidate, provider_version, version):
    base, bound = map(email.message_from_bytes, (original, candidate))
    if (
        base["Name"] != "pyshacl-turbo"
        or bound["Name"] != "pyshacl-turbo"
        or bound["Version"] != version
    ):
        raise ValueError("Candidate distribution metadata differs from its actual namespace")
    expected = []
    for key, value in base.raw_items():
        if key.lower() == "version":
            value = version
        elif key.lower() == "requires-dist" and is_provider(value):
            marker = ";" + value.split(";", 1)[1] if ";" in value else ""
            value = f"rdflib-turbo=={provider_version}" + marker
        expected.append((key, value))
    if expected != list(bound.raw_items()) or base.get_payload() != bound.get_payload():
        raise ValueError("Candidate distribution changed unrelated package metadata")


def verify_archives(base, base_sha, candidate, candidate_sha, provider_version, version, binding):
    before, before_meta = archive_record(base, base_sha)
    after, after_meta = archive_record(candidate, candidate_sha)
    changed = sorted(
        name for name in before.keys() | after.keys() if before.get(name) != after.get(name)
    )
    if before.keys() != after.keys() or set(changed) - {"pyproject.toml", "uv.lock", "PKG-INFO"}:
        raise ValueError("Candidate archive changed original program bytes or source inventory")
    verify_manifest(
        tomllib.loads(before_meta["pyproject.toml"].decode()),
        tomllib.loads(after_meta["pyproject.toml"].decode()),
        provider_version,
        version,
        binding,
    )
    verify_lock(
        tomllib.loads(before_meta["uv.lock"].decode()),
        tomllib.loads(after_meta["uv.lock"].decode()),
    )
    verify_pkginfo(before_meta["PKG-INFO"], after_meta["PKG-INFO"], provider_version, version)
    return {
        "base_sha256": base_sha,
        "candidate_sha256": candidate_sha,
        "changed": changed,
        "files": after,
    }


def artifact_file(root, name):
    relative = PurePosixPath(name)
    if relative.is_absolute() or ".." in relative.parts or "\\" in name:
        raise ValueError("Candidate artifact reference escapes its namespace")
    target = root.joinpath(*relative.parts).resolve(strict=True)
    if not target.is_relative_to(root.resolve()) or not target.is_file():
        raise ValueError("Candidate artifact reference is not an original retained file")
    return target


def file_hash(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def wheel_metadata(path):
    with zipfile.ZipFile(path) as archive:
        names = [name for name in archive.namelist() if name.endswith(".dist-info/METADATA")]
        if len(names) != 1:
            raise ValueError("Expected one candidate wheel metadata record")
        return archive.read(names[0])


def verify_artifacts(path, payload_reader, facade_revision, provider_revision, provider_version):
    record = json.loads(path.read_text())
    if record.get("schema") != 1 or record.get("kind") != "test-only-facade-candidate":
        raise ValueError("Unsupported candidate artifact receipt")
    if (
        record.get("publishable") is not False
        or record.get("artifact_namespace") != "candidate-facade"
    ):
        raise ValueError("Expected an explicitly nonpublishable candidate artifact receipt")
    if (
        record["base_input"]["revision"] != facade_revision
        or record["provider_input"]["revision"] != provider_revision
        or record["provider_version"] != provider_version
    ):
        raise ValueError("Candidate receipt differs from immutable source inputs")
    if not isinstance(record.get("declaration"), dict):
        raise ValueError("Candidate receipt requires original declaration peer authority")
    root = path.resolve().parent
    spec = importlib.util.spec_from_file_location(
        "facade_declaration", Path(__file__).with_name("facade_declaration.py")
    )
    declaration = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(declaration)
    declaration.verify_receipt(
        record["declaration"], artifact_file(root, record["declaration"]["archive"]), archive_record
    )
    if (
        file_hash(artifact_file(root, "declaration-requirements.lock"))
        != record["locked_export_sha256"]
    ):
        raise ValueError("Original owner locked export changed")
    fields = ("base_archive", "base_wheel", "candidate_archive", "candidate_wheel")
    artifacts = {name: artifact_file(root, record[name]) for name in fields}
    for name, original in artifacts.items():
        if file_hash(original) != record[name + "_sha256"]:
            raise ValueError(f"Retained original candidate input changed: {name}")
    before, metadata = archive_record(artifacts["base_archive"], record["base_archive_sha256"])
    original = tomllib.loads(metadata["pyproject.toml"].decode())
    expected_version = candidate_version(
        original["project"]["version"], facade_revision, provider_revision
    )
    if record["candidate_version"] != expected_version:
        raise ValueError("Candidate version does not identify both immutable sources")
    if any(
        record["base_input"]["files"][name] != sha
        for name, sha in before.items()
        if name in record["base_input"]["files"]
    ):
        raise ValueError("Canonical archive differs from its immutable Git source input")
    proof = verify_archives(
        artifacts["base_archive"],
        record["base_archive_sha256"],
        artifacts["candidate_archive"],
        record["candidate_archive_sha256"],
        provider_version,
        expected_version,
        record["binding"],
    )
    if proof != record["archive_proof"]:
        raise ValueError("Candidate archive verification differs from its original receipt")
    if payload_reader(artifacts["base_wheel"]) != payload_reader(artifacts["candidate_wheel"]):
        raise ValueError("Candidate wheel changed canonical program bytes")
    verify_pkginfo(
        wheel_metadata(artifacts["base_wheel"]),
        wheel_metadata(artifacts["candidate_wheel"]),
        provider_version,
        expected_version,
    )
    if (
        file_hash(artifact_file(root, "dependency-binding.patch"))
        != record["dependency_patch_sha256"]
    ):
        raise ValueError("Candidate dependency-only patch changed")
    return {"record": record, "artifacts": artifacts, "proof": proof}


def freeze_source(
    source, wheel, receipt, payload_reader, facade_revision, provider_revision, provider_version
):
    """Attest candidate acceptance bytes using their original archive authority."""
    verified = verify_artifacts(
        receipt, payload_reader, facade_revision, provider_revision, provider_version
    )
    if verified["artifacts"]["candidate_wheel"].resolve() != wheel.resolve():
        raise ValueError("Receipt differs from the explicit candidate wheel")
    spec = importlib.util.spec_from_file_location(
        "coverage_archive", Path(__file__).with_name("coverage_archive.py")
    )
    authority = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(authority)
    record = authority.freeze(
        verified["artifacts"]["candidate_archive"],
        source,
        verified["record"]["candidate_archive_sha256"],
        project="pyshacl-turbo",
    )
    return {
        **record,
        "kind": "candidate-sdist",
        "base_revision": facade_revision,
        "provider_revision": provider_revision,
        "metadata_proof": verified["proof"],
        "receipt_sha256": file_hash(receipt),
    }
