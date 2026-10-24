"""Provider CI consumes one explicitly unpublishable, immutable facade cohort."""

import importlib.util
import json
import subprocess
from pathlib import Path
from types import SimpleNamespace

import pytest

ROOT = Path(__file__).resolve().parents[1]
REVISION = "a" * 40


def helper():
    spec = importlib.util.spec_from_file_location("ci_facade", ROOT / "scripts/ci_facade.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_policy_requires_the_public_repository_and_an_immutable_revision(tmp_path):
    policy = tmp_path / "policy.json"
    document = {
        "facade": {"repository": "https://github.com/legra-ai/pyshacl-turbo", "revision": REVISION},
        "owlrl": {"repository": "https://github.com/legra-ai/owlrl-turbo", "revision": REVISION},
    }
    policy.write_text(json.dumps(document))
    assert helper().policy(policy) == document
    for defect in ("main", "file:///private/source", "/Users/private/checkout"):
        broken = json.loads(json.dumps(document))
        field = "revision" if defect == "main" else "repository"
        broken["facade"][field] = defect
        policy.write_text(json.dumps(broken))
        with pytest.raises(ValueError):
            helper().policy(policy)


def test_evidence_bundle_retains_archives_and_receipts_without_clones(tmp_path):
    module = helper()
    source, output = tmp_path / "candidate-facade-build", tmp_path / "candidate-facade-evidence"
    source.mkdir()
    names = {
        "base_archive": "originals/base.tar.gz",
        "base_wheel": "originals/base.whl",
        "candidate_archive": "candidate-artifacts/candidate.tar.gz",
        "candidate_wheel": "candidate-artifacts/candidate.whl",
    }
    record = {**names, "declaration": {"archive": names["base_archive"]}}
    owl = {"archive": "owl/source.tar.gz", "wheel": "owl/companion.whl"}
    retained = [
        *names.values(),
        *owl.values(),
        "owlrl-inputs.json",
        "candidate-inputs.json",
        "dependency-binding.patch",
        "declaration-requirements.lock",
    ]
    for name in [
        *retained,
        "cohort/candidate-worktree/.git/config",
        "cohort/candidate-source/package.py",
    ]:
        path = source / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(name)
    module.retain_evidence(source, output, record, owl)
    assert {
        path.relative_to(output).as_posix() for path in output.rglob("*") if path.is_file()
    } == set(retained)
    assert all((output / name).read_bytes() == (source / name).read_bytes() for name in retained)


def test_evidence_bundle_rejects_a_path_outside_its_namespace(tmp_path):
    source = tmp_path / "candidate-facade-build"
    source.mkdir()
    for name in (
        "candidate-inputs.json",
        "dependency-binding.patch",
        "declaration-requirements.lock",
        "owlrl-inputs.json",
    ):
        (source / name).write_text("fixture")
    record = dict.fromkeys(
        ("base_archive", "base_wheel", "candidate_archive", "candidate_wheel"), "../private"
    )
    record["declaration"] = {"archive": "../private"}
    with pytest.raises(ValueError):
        helper().retain_evidence(
            source,
            tmp_path / "candidate-facade-evidence",
            record,
            {"archive": "../private", "wheel": "../private"},
        )


def test_consumer_checks_real_provider_inventory_before_extracting_or_exporting(
    tmp_path, monkeypatch
):
    module = helper()
    monkeypatch.setattr(module, "policy", lambda: {"facade": {"revision": REVISION}})
    owner = {"revision": "b" * 40, "files": {"src/lib.rs": "actual"}}
    monkeypatch.setattr(module, "owner_input", lambda: owner)
    manifest = {"project": {"version": "0.2.0a1"}}
    monkeypatch.setattr(module, "owner_manifest", lambda: manifest)
    verified = {
        "record": {"provider_input": {**owner, "files": {"src/lib.rs": "tampered"}}},
        "artifacts": {},
    }
    calls = []

    def verify(*args):
        calls.append(args)
        return verified

    dependencies = {
        "facade_candidate": SimpleNamespace(verify_artifacts=verify),
        "native_coverage": SimpleNamespace(facade_payload=object()),
    }
    monkeypatch.setattr(module, "load", lambda name: dependencies[name])
    bundle = tmp_path / "candidate-facade-evidence"
    bundle.mkdir()
    with pytest.raises(ValueError, match="provider source"):
        module.consume(bundle)
    assert calls[0][2:] == (REVISION, owner["revision"], "0.2.0a1")


def test_environment_lines_cannot_inject_other_github_commands(tmp_path):
    destination = tmp_path / "environment"
    module = helper()
    with pytest.raises(ValueError):
        module.write_environment(destination, {"SHACL_WHEEL": "good\nOTHER=bad"})
    assert not destination.exists()
    module.write_environment(destination, {"SHACL_WHEEL": "/tmp/original.whl"})
    assert destination.read_text() == "SHACL_WHEEL=/tmp/original.whl\n"


def test_original_archive_sha256_environment_name_is_valid(tmp_path):
    destination = tmp_path / "environment"
    helper().write_environment(destination, {"OWLRL_SHA256": "a" * 64})
    assert destination.read_text() == "OWLRL_SHA256=" + "a" * 64 + "\n"


def test_consumer_git_launch_does_not_use_the_posix_only_coverage_launcher(monkeypatch):
    module = helper()
    calls = []

    def launch(command, **kwargs):
        calls.append((command, kwargs))
        return subprocess.CompletedProcess(command, 0, stdout="immutable-revision\n")

    monkeypatch.setattr(module.subprocess, "run", launch)
    assert module.run(["git", "rev-parse", "HEAD"], capture=True) == "immutable-revision\n"
    assert calls[0][1] == {"check": True, "text": True, "stdout": subprocess.PIPE}


def test_prepare_rejects_an_unavailable_owl_revision_before_building_any_facade(
    tmp_path, monkeypatch
):
    module = helper()
    selected = {"facade": {"revision": REVISION}, "owlrl": {"revision": "c" * 40}}
    monkeypatch.setattr(module, "policy", lambda: selected)
    monkeypatch.setattr(module, "owner_input", lambda: {"revision": "b" * 40})
    calls = []

    def checkout(peer, destination):
        calls.append(("checkout", peer["revision"]))
        if peer == selected["owlrl"]:
            raise subprocess.CalledProcessError(128, ["git", "checkout", peer["revision"]])

    def build(arguments):
        calls.append(("build", arguments))
        raise AssertionError("Unavailable OWL authority must be detected before pure builds")

    builder = SimpleNamespace(build=build, freeze=lambda *args: {"revision": args[1], "files": {}})
    monkeypatch.setattr(module, "checkout", checkout)
    monkeypatch.setattr(module, "load", lambda name: builder)
    with pytest.raises(subprocess.CalledProcessError):
        module.prepare(tmp_path / "candidate-facade-evidence")
    assert calls == [("checkout", REVISION), ("checkout", "c" * 40)]


def test_prepare_rechecks_both_immutable_peers_after_pure_artifact_construction(
    tmp_path, monkeypatch
):
    module = helper()
    selected = {"facade": {"revision": REVISION}, "owlrl": {"revision": "c" * 40}}
    monkeypatch.setattr(module, "policy", lambda: selected)
    monkeypatch.setattr(module, "owner_input", lambda: {"revision": "b" * 40})
    calls = []
    changed = False

    def freeze(source, revision, project, run):
        calls.append(("freeze", project))
        return {"revision": revision, "files": {"source.py": "changed" if changed else "original"}}

    def build(arguments):
        nonlocal changed
        calls.append(("build", arguments.facade_revision))
        changed = True
        return {}

    monkeypatch.setattr(
        module, "checkout", lambda peer, destination: calls.append(("checkout", peer["revision"]))
    )
    monkeypatch.setattr(module, "retain_evidence", lambda *args: None)
    monkeypatch.setattr(
        module,
        "load",
        lambda name: SimpleNamespace(build=build, freeze=freeze, prepare=lambda *args: {}),
    )
    with pytest.raises(ValueError, match="peer source"):
        module.prepare(tmp_path / "candidate-facade-evidence")
    assert calls.index(("freeze", "owlrl-turbo")) < calls.index(("build", REVISION))


def test_single_portable_bundle_includes_original_owl_archive_wheel_and_receipt(tmp_path):
    module = helper()
    source, output = tmp_path / "candidate-facade-build", tmp_path / "candidate-facade-evidence"
    source.mkdir()
    record = {
        name: name + ".original"
        for name in ("base_archive", "base_wheel", "candidate_archive", "candidate_wheel")
    }
    record["declaration"] = {"archive": record["base_archive"]}
    owl = {"archive": "owl-originals/source.tar.gz", "wheel": "owl-originals/owl.whl"}
    retained = {
        *(
            record[name]
            for name in ("base_archive", "base_wheel", "candidate_archive", "candidate_wheel")
        ),
        owl["archive"],
        owl["wheel"],
        "candidate-inputs.json",
        "dependency-binding.patch",
        "declaration-requirements.lock",
        "owlrl-inputs.json",
    }
    for name in retained:
        path = source / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(name)
    module.retain_evidence(source, output, record, owl)
    assert {
        path.relative_to(output).as_posix() for path in output.rglob("*") if path.is_file()
    } == retained


def test_every_consumer_receives_the_same_original_owl_without_fetching_or_building(
    tmp_path, monkeypatch
):
    module = helper()
    bundle = tmp_path / "candidate-facade-evidence"
    bundle.mkdir()
    owner = {"revision": "b" * 40, "files": {"source": "actual"}}
    record = {
        "provider_input": owner,
        "candidate_archive_sha256": "d" * 64,
        "declaration": {"archive": "base.tar.gz", "archive_sha256": "e" * 64},
    }
    artifacts = {
        "candidate_archive": bundle / "candidate.tar.gz",
        "candidate_wheel": bundle / "candidate.whl",
    }
    owl = {
        "source": bundle / "owlrl-source",
        "wheel": bundle / "owl.whl",
        "archive": bundle / "owl.tar.gz",
        "record": {"archive_sha256": "f" * 64},
    }
    dependencies = {
        "facade_candidate": SimpleNamespace(
            verify_artifacts=lambda *args: {"record": record, "artifacts": artifacts},
            artifact_file=lambda root, name: root / name,
        ),
        "native_coverage": SimpleNamespace(facade_payload=object()),
        "build_facade_candidate": SimpleNamespace(freeze=lambda *args: owner),
        "owl_companion_inputs": SimpleNamespace(
            verify=lambda *args: owl, locked_version=lambda root: "0.1.0a3"
        ),
    }
    monkeypatch.setattr(module, "load", lambda name: dependencies[name])
    monkeypatch.setattr(module, "owner_input", lambda: owner)
    monkeypatch.setattr(module, "owner_manifest", lambda: {"project": {"version": "0.2.0a18"}})
    monkeypatch.setattr(
        module,
        "policy",
        lambda: {"facade": {"revision": REVISION}, "owlrl": {"revision": "c" * 40}},
    )
    monkeypatch.setattr(module, "extract_original", lambda *args: None)
    monkeypatch.setattr(module, "declaration_peer", lambda manifest: tmp_path / "peer")
    monkeypatch.setattr(
        module, "checkout", lambda *args: pytest.fail("consumer fetched an OWL source fallback")
    )
    environment = module.consume(bundle)
    assert environment["OWLRL_WHEEL"] == str(owl["wheel"])
    assert environment["OWLRL_SOURCE"] == str(owl["source"])
    assert environment["OWLRL_ARCHIVE"] == str(owl["archive"])
    assert environment["OWLRL_SHA256"] == "f" * 64
