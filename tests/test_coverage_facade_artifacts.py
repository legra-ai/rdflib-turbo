"""Combined native measurements require a pure, byte-verified installed facade."""

import importlib.util
import json
import zipfile
from pathlib import Path

import pytest


def harness():
    script = Path(__file__).resolve().parents[1] / "scripts/native_coverage.py"
    spec = importlib.util.spec_from_file_location("facade_coverage", script)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def wheel(tmp_path, extra=None, pure=True):
    path = tmp_path / "pyshacl_turbo-0.0.0-py3-none-any.whl"
    members = {
        "pyshacl_turbo/__init__.py": b"",
        "pyshacl_turbo/_native.py": b"from rdflib_turbo._native import shacl\n",
        "pyshacl_turbo-0.0.0.dist-info/WHEEL": (
            f"Wheel-Version: 1.0\nRoot-Is-Purelib: {str(pure).lower()}\nTag: py3-none-any\n"
        ).encode(),
    }
    members.update(extra or {})
    with zipfile.ZipFile(path, "w") as archive:
        for name, content in members.items():
            archive.writestr(name, content)
    return path, members


def test_facade_coverage_preserves_every_package_payload_hash(tmp_path):
    module = harness()
    path, members = wheel(tmp_path, {"pyshacl_turbo/py.typed": b""})
    measured = module.facade_payload(path)
    assert measured == {
        name: module.hashlib.sha256(content).hexdigest()
        for name, content in members.items()
        if name.startswith("pyshacl_turbo/")
    }


def test_portable_source_payload_verification_never_launches_native_coverage(tmp_path, monkeypatch):
    module = harness()
    path, members = wheel(tmp_path)
    source = source_fixture(tmp_path, members)
    monkeypatch.setattr(module.sys, "platform", "win32")
    monkeypatch.setattr(
        module, "run", lambda *args, **kwargs: pytest.fail("Native coverage command")
    )
    assert module.facade_source_payload(source, path) == module.facade_payload(path)


@pytest.mark.parametrize("name", ["_native.so", "helper.pyd", "lib.so.1", "helper.dll"])
def test_facade_coverage_rejects_a_second_native_runtime(tmp_path, name):
    path, _ = wheel(tmp_path, {f"pyshacl_turbo/{name}": b"second runtime"})
    with pytest.raises(RuntimeError, match="native"):
        harness().facade_payload(path)


def test_facade_coverage_rejects_native_wheel_metadata(tmp_path):
    path, _ = wheel(tmp_path, pure=False)
    with pytest.raises(RuntimeError, match="pure"):
        harness().facade_payload(path)


@pytest.mark.parametrize("damage", ["outside", "changed", "extra"])
def test_installed_facade_rejects_source_imports_and_payload_drift(tmp_path, monkeypatch, damage):
    module = harness()
    path, _ = wheel(tmp_path)
    venv = tmp_path / "venv"
    actual = {
        "package": str(venv / "lib/pyshacl_turbo"),
        "payload": module.facade_payload(path),
        "engine_fingerprint": "test fingerprint",
    }
    if damage == "outside":
        actual["package"] = str(tmp_path / "source/pyshacl_turbo")
    elif damage == "changed":
        actual["payload"]["pyshacl_turbo/_native.py"] = "changed"
    else:
        actual["payload"]["pyshacl_turbo/extra.py"] = "unexpected"
    monkeypatch.setattr(module, "run", lambda *args, **kwargs: json.dumps(actual))
    with pytest.raises(RuntimeError, match=r"outside|differs"):
        module.verify_installed_facade(path, ["python"], venv, {})


def source_fixture(tmp_path, members):
    source = tmp_path / "facade-source"
    (source / "tests").mkdir(parents=True)
    (source / "tests/test_acceptance.py").write_text("def test_public_contract(): pass\n")
    (source / "pyproject.toml").write_text('[project]\nname="pyshacl-turbo"\nversion="0.0.0"\n')
    for name, content in members.items():
        if name.startswith("pyshacl_turbo/"):
            target = source / "python" / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(content)
    return source


def frozen_git(module, source, monkeypatch):
    tracked = sorted(
        path.relative_to(source).as_posix() for path in source.rglob("*") if path.is_file()
    )

    def run(arguments, *args, **kwargs):
        command = list(map(str, arguments))
        assert command[:3] == ["git", "-C", str(source)]
        if command[3] == "status":
            return ""
        if command[3] == "rev-parse":
            return "a" * 40 + "\n"
        assert command[3:] == ["ls-files", "-z"]
        return "\0".join(tracked) + "\0"

    monkeypatch.setattr(module, "run", run)


def test_explicit_facade_inputs_preserve_source_acceptance_and_wheel_identity(
    tmp_path, monkeypatch
):
    module = harness()
    path, members = wheel(tmp_path, {"pyshacl_turbo/py.typed": b""})
    source = source_fixture(tmp_path, members)
    frozen_git(module, source, monkeypatch)
    record = module.freeze_facade_inputs(source, path)
    assert record["revision"] == "a" * 40
    assert record["wheel_sha256"] == module.digest(path)
    assert record["package_payload"] == module.facade_payload(path)
    assert "tests/test_acceptance.py" in record["source_sha256"]
    module.verify_facade_inputs(source, path, record)


@pytest.mark.parametrize("damage", ["changed", "extra", "missing", "outside"])
def test_frozen_source_package_must_exactly_match_explicit_facade_wheel(
    tmp_path, monkeypatch, damage
):
    module = harness()
    path, members = wheel(tmp_path)
    source = source_fixture(tmp_path, members)
    package = source / "python/pyshacl_turbo"
    if damage == "changed":
        (package / "_native.py").write_text("changed")
    elif damage == "extra":
        (package / "extra.py").write_text("unexpected")
    elif damage == "missing":
        (package / "_native.py").unlink()
    else:
        outside = tmp_path / "outside.py"
        outside.write_text("outside")
        (package / "_native.py").unlink()
        (package / "_native.py").symlink_to(outside)
    frozen_git(module, source, monkeypatch)
    with pytest.raises(RuntimeError, match=r"payload|outside"):
        module.freeze_facade_inputs(source, path)


@pytest.mark.parametrize("damage", ["acceptance", "wheel", "new-source"])
def test_frozen_inputs_cannot_change_during_measurement(tmp_path, monkeypatch, damage):
    module = harness()
    path, members = wheel(tmp_path)
    source = source_fixture(tmp_path, members)
    frozen_git(module, source, monkeypatch)
    record = module.freeze_facade_inputs(source, path)
    if damage == "acceptance":
        (source / "tests/test_acceptance.py").write_text("changed")
    elif damage == "wheel":
        with zipfile.ZipFile(path, "a") as archive:
            archive.writestr("pyshacl_turbo/unexpected.py", "changed")
    else:
        (source / "tests/new.py").write_text("changed")
        frozen_git(module, source, monkeypatch)
    with pytest.raises(RuntimeError, match=r"changed|payload"):
        module.verify_facade_inputs(source, path, record)


def test_missing_explicit_wheel_fails_before_any_native_command(tmp_path, monkeypatch):
    module = harness()
    _, members = wheel(tmp_path)
    source = source_fixture(tmp_path, members)
    monkeypatch.setattr(
        module.sys,
        "argv",
        ["native_coverage.py", "--output", str(tmp_path / "out"), "--shacl-source", str(source)],
    )
    monkeypatch.setattr(
        module, "run", lambda *args, **kwargs: pytest.fail("native command before required wheel")
    )
    with pytest.raises(ValueError, match="--shacl-wheel"):
        module.main()


def test_candidate_facade_freeze_retains_archive_authority_and_never_queries_git(
    tmp_path, monkeypatch
):
    module = harness()
    path, members = wheel(tmp_path)
    source = source_fixture(tmp_path, members)
    files = {str(p.relative_to(source)): module.digest(p) for p in source.rglob("*") if p.is_file()}
    authority = {
        "kind": "candidate-sdist",
        "sha256": "c" * 64,
        "base_revision": "a" * 40,
        "files": files,
    }
    candidate = {
        "receipt": tmp_path / "candidate-inputs.json",
        "facade_revision": "a" * 40,
        "provider_revision": "b" * 40,
        "provider_version": "0.2.0a9",
    }
    monkeypatch.setattr(module, "candidate_facade_authority", lambda *args, **kwargs: authority)
    monkeypatch.setattr(
        module, "run", lambda *args, **kwargs: pytest.fail("Candidate source has no Git authority")
    )
    record = module.freeze_facade_inputs(source, path, candidate=candidate)
    assert record["revision"] == "candidate-sdist-sha256:" + "c" * 64
    assert record["source_authority"] == authority
    assert record["source_sha256"] == files
    module.verify_facade_inputs(source, path, record, candidate=candidate)
    (source / "python/pyshacl_turbo/_native.py").write_text("changed")
    with pytest.raises(RuntimeError, match="payload"):
        module.verify_facade_inputs(source, path, record, candidate=candidate)


@pytest.mark.parametrize("receipt,base", [(None, "a" * 40), ("candidate.json", None)])
def test_candidate_authority_requires_complete_explicit_pair(receipt, base):
    with pytest.raises(ValueError, match="Both candidate receipt and base revision"):
        harness().candidate_parameters(
            {"kind": "git", "revision": "b" * 40}, receipt, base, "1.0.0"
        )


def test_candidate_authority_cannot_enter_published_provider_path(tmp_path):
    module = harness()
    with pytest.raises(ValueError, match="immutable Git provider"):
        module.candidate_parameters(
            {"kind": "published-sdist"}, tmp_path / "receipt", "a" * 40, "1.0.0"
        )
    with pytest.raises(ValueError, match="immutable facade base"):
        module.candidate_parameters(
            {"kind": "git", "revision": "b" * 40}, tmp_path / "receipt", "main", "1.0.0"
        )
    assert module.candidate_parameters({"kind": "published-sdist"}, None, None, "1.0.0") is None
    assert module.candidate_parameters(
        {"kind": "git", "revision": "b" * 40}, tmp_path / "receipt", "a" * 40, "1.0.0"
    ) == {
        "receipt": tmp_path / "receipt",
        "facade_revision": "a" * 40,
        "provider_revision": "b" * 40,
        "provider_version": "1.0.0",
    }


def test_frozen_git_fixture_emits_git_paths_on_windows(monkeypatch):
    from pathlib import PureWindowsPath
    from types import SimpleNamespace

    file = SimpleNamespace(
        is_file=lambda: True, relative_to=lambda source: PureWindowsPath("tests/test_acceptance.py")
    )
    source = SimpleNamespace(rglob=lambda pattern: [file])
    module = SimpleNamespace(run=None)
    frozen_git(module, source, monkeypatch)
    assert module.run(["git", "-C", str(source), "ls-files", "-z"]) == "tests/test_acceptance.py\0"
