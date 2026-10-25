"""Installed payload attestations retain source/model authority without invented Git revisions."""

import hashlib
import importlib.util
import json
import tarfile
from pathlib import Path
from types import SimpleNamespace
from zipfile import ZipFile

import pytest

ROOT = Path(__file__).resolve().parents[1]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


@pytest.fixture
def arm(tmp_path, monkeypatch):
    fixture = load("attest_fixture", Path(__file__).with_name("comparison_owner_fixture.py"))
    owner = fixture.make_owner(tmp_path)
    commit = fixture.commit_source(owner["source"])
    base = tmp_path / "base"
    (base / "sdist").mkdir(parents=True)
    archive = base / "sdist/rdflib_turbo-0.2.0a19.tar.gz"
    source_tool = load("attest_source", ROOT / "scripts/comparison_source.py")
    source_files = source_tool.freeze(owner["source"])
    with tarfile.open(archive, "w:gz") as bundle:
        for name in source_files:
            bundle.add(owner["source"] / name, arcname="rdflib_turbo-0.2.0a19/" + name)
    (base / "extracted").mkdir()
    with tarfile.open(archive) as bundle:
        bundle.extractall(base / "extracted", filter="data")
    source = base / "extracted/rdflib_turbo-0.2.0a19"
    for package in owner["metadata"]["packages"]:
        path = Path(package["manifest_path"])
        if path.is_relative_to(owner["source"]):
            package["manifest_path"] = str(source / path.relative_to(owner["source"]))
    metadata = base / "cargo-metadata.json"
    metadata.write_text(json.dumps(owner["metadata"]))
    model = load("attest_model", ROOT / "scripts/comparison_model.py").prepare(
        source, owner["metadata"], base / "model"
    )
    identity = load("attest_identity", ROOT / "scripts/comparison_identity.py").derive(
        source, owner["metadata"]
    )
    environment = tmp_path / "environment"
    site = environment / "lib/python3.13/site-packages"
    modules, wheels = {}, {}
    for name in ("rdflib_turbo", "owlrl_turbo"):
        package = site / name
        package.mkdir(parents=True)
        payload = {"__init__.py": b"# immutable fixture\n"}
        if name == "rdflib_turbo":
            payload["_native.so"] = b"unit boundary only; never a native artifact"
        wheel = base / "wheels" / (name + "-fixture.whl")
        wheel.parent.mkdir(exist_ok=True)
        with ZipFile(wheel, "w") as bundle:
            for filename, data in payload.items():
                (package / filename).write_bytes(data)
                bundle.writestr(name + "/" + filename, data)
        modules[name] = SimpleNamespace(__file__=str(package / "__init__.py"))
        wheels[name] = wheel
    native = SimpleNamespace(
        __file__=str(site / "rdflib_turbo/_native.so"),
        SHACL_ENGINE_SOURCE="crates/engine",
        SHACL_ENGINE_FINGERPRINT=identity["engine_fingerprint"],
        shacl=SimpleNamespace(SHACL_ENGINE_FINGERPRINT=identity["engine_fingerprint"]),
    )
    modules["rdflib_turbo._native"] = native
    checker = load("attestation_under_test", ROOT / "scripts/comparison_attest.py")
    monkeypatch.setattr(
        checker, "sys", SimpleNamespace(version="3.13.16 (fixture)", prefix=str(environment))
    )
    monkeypatch.setattr(checker, "version", lambda name: "7.6.0")
    monkeypatch.setattr(checker, "find_spec", lambda name: None)
    monkeypatch.setattr(
        checker,
        "importlib",
        SimpleNamespace(import_module=lambda name: modules[name], util=importlib.util),
    )
    receipt = {
        "commit": commit,
        "source_checkout": str(owner["source"]),
        "source_files": source_files,
        "archive": str(archive),
        "archive_sha256": source_tool.sha(archive),
        "archive_source": str(source),
        "archive_files": source_tool.archive_files(archive, source.name),
        "cargo_metadata": str(metadata),
        "cargo_metadata_sha256": source_tool.sha(metadata),
        "expected_identity": identity,
        "model_source": model,
        "wheel": str(wheels["rdflib_turbo"]),
        "wheel_sha256": source_tool.sha(wheels["rdflib_turbo"]),
        "owlrl_wheel": str(wheels["owlrl_turbo"]),
        "owlrl_wheel_sha256": source_tool.sha(wheels["owlrl_turbo"]),
    }
    return {"checker": checker, "receipt": receipt, "native": native, "owner": owner}


def test_actual_local_source_exports_need_no_fake_git_revision(arm):
    proof = arm["checker"].attest(arm["receipt"])
    assert proof["engine"] == arm["receipt"]["expected_identity"]
    assert proof["model"] == arm["receipt"]["model_source"]
    assert "engine_revision" not in proof["engine"]


@pytest.mark.parametrize(
    "tamper",
    (
        "source",
        "compiled-fp",
        "standalone-fp",
        "source-count",
        "seventh-fork",
        "archive-inventory",
        "git-source",
        "metadata-owner",
    ),
)
def test_source_fork_model_and_compiled_authorities_cannot_diverge(arm, tamper):
    receipt, native = arm["receipt"], arm["native"]
    if tamper == "source":
        native.SHACL_ENGINE_SOURCE = "elsewhere"
    elif tamper == "compiled-fp":
        native.SHACL_ENGINE_FINGERPRINT = "0" * 64
    elif tamper == "standalone-fp":
        native.shacl.SHACL_ENGINE_FINGERPRINT = "0" * 64
    elif tamper == "source-count":
        receipt["expected_identity"]["engine_source_count"] += 1
    elif tamper == "seventh-fork":
        receipt["expected_identity"]["dependency_fingerprints"]["fancy-regex"] = "0" * 64
    elif tamper == "archive-inventory":
        del receipt["archive_files"]["crates/engine/src/lib.rs"]
    elif tamper == "git-source":
        (arm["owner"]["source"] / "crates/engine/src/lib.rs").write_text("changed")
    else:
        path = Path(receipt["cargo_metadata"])
        value = json.loads(path.read_text())
        next(p for p in value["packages"] if p["id"] == "engine")["manifest_path"] = (
            "/outside/Cargo.toml"
        )
        path.write_text(json.dumps(value))
        receipt["cargo_metadata_sha256"] = hashlib.sha256(path.read_bytes()).hexdigest()
    with pytest.raises(ValueError):
        arm["checker"].attest(receipt)


def test_loaded_native_library_cannot_differ_from_the_verified_wheel_location(arm, tmp_path):
    outside = tmp_path / "foreign_native.so"
    outside.write_bytes(Path(arm["native"].__file__).read_bytes())
    arm["native"].__file__ = str(outside)
    with pytest.raises(ValueError, match="Loaded native library"):
        arm["checker"].attest(arm["receipt"])
