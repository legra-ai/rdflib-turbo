"""Retain unreleased OWL test artifacts without identifying them as PyPI releases."""

import importlib.util
import json
import tomllib
from pathlib import Path


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def verify_source_files(files, source):
    if any(source["files"].get(name) != sha for name, sha in files.items() if name != "PKG-INFO"):
        raise ValueError("OWL source archive differs from its exact immutable Git input")


def prepare(source, selected, frozen, output, python, run):
    builder, companion = load("build_facade_candidate"), load("pure_companion")
    archive, wheel = builder.build_pair(source, output / "owl-originals", python, run)
    extracted = output / "owl-original-source"
    builder.extract_candidate(archive, extracted)
    sha = companion.digest(archive)
    authority = load("coverage_archive").freeze(archive, extracted, sha, project="owlrl-turbo")
    verify_source_files(authority["files"], frozen)
    record = {
        "schema": 1,
        "kind": "immutable-public-owl-test-companion",
        "publishable": False,
        "repository": selected["repository"],
        "source": frozen,
        "archive": archive.relative_to(output).as_posix(),
        "archive_sha256": sha,
        "wheel": wheel.relative_to(output).as_posix(),
        "wheel_sha256": companion.digest(wheel),
        "archive_files": authority["files"],
        "package_payload": companion.verify_payload(source, wheel),
    }
    (output / "owlrl-inputs.json").write_text(json.dumps(record, indent=2, sort_keys=True) + "\n")
    return record


def verify(bundle, selected, locked_version):
    record = json.loads((bundle / "owlrl-inputs.json").read_text())
    if (
        record.get("schema") != 1
        or record.get("kind") != "immutable-public-owl-test-companion"
        or record.get("publishable") is not False
        or record["repository"] != selected["repository"]
        or record["source"]["revision"] != selected["revision"]
    ):
        raise ValueError("OWL companion receipt differs from the immutable public test input")
    rules, companion = load("facade_candidate"), load("pure_companion")
    archive, wheel = [rules.artifact_file(bundle, record[name]) for name in ("archive", "wheel")]
    if (
        companion.digest(archive) != record["archive_sha256"]
        or companion.digest(wheel) != record["wheel_sha256"]
    ):
        raise ValueError("Original OWL companion archive or wheel changed")
    source = bundle / "owlrl-source"
    if not source.exists():
        load("build_facade_candidate").extract_candidate(archive, source)
    frozen = load("coverage_archive").freeze(
        archive, source, record["archive_sha256"], project="owlrl-turbo"
    )
    if frozen["files"] != record["archive_files"] or frozen["version"] != locked_version:
        raise ValueError("OWL archive differs from its recorded source or declared locked version")
    verify_source_files(frozen["files"], record["source"])
    payload = companion.verify_payload(source, wheel)
    if payload != record["package_payload"]:
        raise ValueError("Original OWL companion package payload changed")
    return {
        "record": record,
        "archive": archive,
        "wheel": wheel,
        "source": source,
        "inputs": {"wheel_sha256": record["wheel_sha256"], "package_payload": payload},
    }


def locked_version(owner):
    lock = tomllib.loads((owner / "uv.lock").read_text())
    packages = [package for package in lock["package"] if package["name"] == "owlrl-turbo"]
    if len(packages) != 1 or packages[0]["source"] != {"registry": "https://pypi.org/simple"}:
        raise ValueError("Expected one declared registry OWL test dependency")
    return packages[0]["version"]
