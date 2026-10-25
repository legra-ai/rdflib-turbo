"""Portable pure YAML dependency authority rejects altered original inputs."""

import hashlib
import importlib.util
import io
import json
import os
import shutil
import sys
import tarfile
from pathlib import Path
from types import SimpleNamespace
from zipfile import ZipFile

import pytest

SCRIPT = Path(__file__).resolve().parents[1] / "scripts/yaml_test_inputs.py"
RUNTIME = {
    "yaml/__init__.py": b"__version__ = '6.0.3'\n",
    "yaml/reader.py": b"# reader\n",
    "_yaml/__init__.py": b"# optional compatibility shim\n",
}


def digest(data):
    return hashlib.sha256(data).hexdigest()


@pytest.fixture
def helper():
    spec = importlib.util.spec_from_file_location("yaml_test_inputs", SCRIPT)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def source_archive(extra=None):
    output = io.BytesIO()
    with tarfile.open(fileobj=output, mode="w:gz") as stream:
        files = {
            "pyproject.toml": b"[build-system]\nrequires = ['setuptools']\n",
            **{"lib/" + p: data for p, data in RUNTIME.items()},
        }
        files.update(extra or {})
        for path, data in files.items():
            member = tarfile.TarInfo("pyyaml-6.0.3/" + path)
            member.size = len(data)
            stream.addfile(member, io.BytesIO(data))
    return output.getvalue()


def wheel(path, *, changes=None, tag="py3-none-any", version="6.0.3", pure="true"):
    payload = {**RUNTIME, **(changes or {})}
    with ZipFile(path, "w") as stream:
        for name, data in payload.items():
            stream.writestr(name, data)
        prefix = "pyyaml-6.0.3.dist-info/"
        stream.writestr(
            prefix + "METADATA", f"Metadata-Version: 2.4\nName: PyYAML\nVersion: {version}\n"
        )
        stream.writestr(
            prefix + "WHEEL", f"Wheel-Version: 1.0\nRoot-Is-Purelib: {pure}\nTag: {tag}\n"
        )


def locked(owner, archive, *, version="6.0.3", duplicate=False):
    owner.mkdir()
    entry = f'[[package]]\nname = "pyyaml"\nversion = "{version}"\nsource = {{ registry = "https://pypi.org/simple" }}\nsdist = {{ url = "https://files.pythonhosted.org/packages/pyyaml-6.0.3.tar.gz", hash = "sha256:{digest(archive)}", size = {len(archive)} }}\n'
    (owner / "uv.lock").write_text(entry * (2 if duplicate else 1))


@pytest.fixture
def prepared(helper, tmp_path, monkeypatch):
    archive = source_archive()
    owner = tmp_path / "owner"
    locked(owner, archive)
    commands = []

    def download(url, destination):
        assert url == "https://files.pythonhosted.org/packages/pyyaml-6.0.3.tar.gz"
        destination.write_bytes(archive)

    def run(command, directory, environment):
        commands.append(command)
        if command == ["uv", "--version"]:
            return "uv 0.13.1 (fixture)\n"
        if "-c" in command:
            return "3.13.16\n"
        assert command[:3] == ["uv", "build", "--wheel"]
        assert "--no-sources" in command
        assert command[-1].startswith("originals/")
        assert environment["PYYAML_FORCE_LIBYAML"] == "0"
        assert Path(environment["TMPDIR"]).is_relative_to(directory)
        assert not (directory / "source/build").exists()
        target = directory / "wheels/pyyaml-6.0.3-py3-none-any.whl"
        target.parent.mkdir(exist_ok=True)
        wheel(target)
        return ""

    monkeypatch.setattr(helper, "_download", download)
    monkeypatch.setattr(helper, "_run", run)
    output = tmp_path / "bundle"
    record = helper.prepare(owner, output, "3.13.16")
    return helper, owner, output, record, commands


def test_prepare_and_relocated_verify_keep_original_archive_and_full_runtime(prepared, tmp_path):
    helper, owner, output, record, commands = prepared
    assert record["schema"] == 1 and record["kind"] == "test-only-yaml-dependency"
    assert record["publishable"] is False
    assert record["package_payload"] == {p: digest(data) for p, data in RUNTIME.items()}
    assert str(output) not in json.dumps(record["recipe"])
    assert record["provenance"]["cwd"] == str(output)
    relocated = tmp_path / "relocated"
    shutil.copytree(output, relocated)
    shutil.rmtree(output)
    checked = helper.verify(owner, relocated / "yaml-inputs.json")
    assert checked["record"] == record
    assert checked["source"] == relocated / record["source"]
    assert digest(checked["archive"].read_bytes()) == record["archive_sha256"]
    assert len([c for c in commands if c[:2] == ["uv", "build"]]) == 1


@pytest.mark.parametrize("version,duplicate", [("6.0.2", False), ("6.0.3", True)])
def test_prepare_rejects_nonunique_or_changed_locked_version(helper, tmp_path, version, duplicate):
    owner = tmp_path / "owner"
    locked(owner, source_archive(), version=version, duplicate=duplicate)
    with pytest.raises(ValueError):
        helper.prepare(owner, tmp_path / "bundle", "3.13.16")


@pytest.mark.parametrize("path", ["../escape.py", "/absolute.py", "a\\bad.py", "C:drive.py"])
def test_prepare_rejects_unsafe_original_archive(helper, tmp_path, monkeypatch, path):
    archive = source_archive({path: b"bad"})
    owner = tmp_path / "owner"
    locked(owner, archive)
    monkeypatch.setattr(
        helper, "_download", lambda url, destination: destination.write_bytes(archive)
    )
    with pytest.raises(ValueError):
        helper.prepare(owner, tmp_path / "bundle", "3.13.16")


@pytest.mark.parametrize(
    "mutation",
    [
        "archive",
        "source",
        "source-extra",
        "lock",
        "record-payload",
        "record-schema",
        "schema-bool",
        "record-path",
        "record-provenance",
        "python-command",
    ],
)
def test_verify_rejects_altered_authority_inputs(prepared, mutation):
    helper, owner, output, record, _ = prepared
    if mutation == "archive":
        (output / record["archive"]).write_bytes(b"altered")
        record["archive_sha256"] = digest(b"altered")
    elif mutation == "source":
        (output / record["source"] / "lib/yaml/reader.py").write_bytes(b"altered")
    elif mutation == "source-extra":
        (output / record["source"] / "extra.py").write_bytes(b"extra")
    elif mutation == "lock":
        with (owner / "uv.lock").open("a") as stream:
            stream.write("\n# altered\n")
    elif mutation == "record-payload":
        record["package_payload"]["yaml/reader.py"] = "0" * 64
    elif mutation == "record-schema":
        record["publishable"] = True
    elif mutation == "schema-bool":
        record["schema"] = True
    elif mutation == "record-path":
        record["source"] = "../outside"
    elif mutation == "record-provenance":
        record["provenance"]["environment"]["PYYAML_FORCE_LIBYAML"] = "1"
    else:
        record["provenance"]["python_command"] = ["python", "unverified.py"]
    (output / "yaml-inputs.json").write_text(json.dumps(record))
    with pytest.raises((ValueError, FileNotFoundError)):
        helper.verify(owner, output / "yaml-inputs.json")


@pytest.mark.skipif(not hasattr(os, "mkfifo"), reason="Unix nonregular source entry")
def test_verify_rejects_nonregular_reference_source_entry(prepared):
    helper, owner, output, record, _ = prepared
    os.mkfifo(output / record["source"] / "extra-fifo")
    with pytest.raises(ValueError, match="regular"):
        helper.verify(owner, output / "yaml-inputs.json")


@pytest.mark.parametrize(
    "mutation",
    [
        "payload",
        "extra",
        "dso",
        "tag",
        "version",
        "pure",
        "filename",
        "duplicate-version",
        "duplicate-pure",
    ],
)
def test_verify_rejects_wheel_mutations_even_with_updated_digest(prepared, mutation):
    helper, owner, output, record, _ = prepared
    target = output / record["wheel"]
    kwargs = {"changes": {"yaml/reader.py": b"altered"}} if mutation == "payload" else {}
    if mutation == "extra":
        kwargs["changes"] = {"yaml/extra.py": b"extra"}
    if mutation == "dso":
        kwargs["changes"] = {"yaml/_yaml.so": b"native"}
    if mutation == "tag":
        kwargs["tag"] = "cp313-cp313-linux_x86_64"
    if mutation == "version":
        kwargs["version"] = "6.0.4"
    if mutation == "pure":
        kwargs["pure"] = "false"
    if mutation == "duplicate-version":
        kwargs["version"] = "6.0.3\nVersion: 6.0.4"
    if mutation == "duplicate-pure":
        kwargs["pure"] = "true\nRoot-Is-Purelib: false"
    if mutation == "filename":
        target = target.with_name("other-0-py3-none-any.whl")
        record["wheel"] = "wheels/" + target.name
    wheel(target, **kwargs)
    record["wheel_sha256"] = digest(target.read_bytes())
    (output / "yaml-inputs.json").write_text(json.dumps(record))
    with pytest.raises(ValueError):
        helper.verify(owner, output / "yaml-inputs.json")


def test_loaded_checks_full_payload_without_importing_optional_c_modules(
    prepared, tmp_path, monkeypatch
):
    helper, _, output, record, _ = prepared
    venv = tmp_path / "venv"
    site = venv / "site-packages"
    for name, data in RUNTIME.items():
        path = site / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
    module = SimpleNamespace(__file__=str(site / "yaml/__init__.py"), __version__="6.0.3")
    monkeypatch.setitem(sys.modules, "yaml", module)
    imported = []

    def import_module(name):
        imported.append(name)
        assert name == "yaml", "Optional C modules must not be imported for a pure wheel"
        return module

    monkeypatch.setattr(helper.importlib, "import_module", import_module)
    proof = helper.verify_loaded(output / record["wheel"], venv)
    assert proof["payload"] == record["package_payload"]
    assert imported == ["yaml"]
    (site / "_yaml/extra.py").write_text("extra")
    with pytest.raises(ValueError):
        helper.verify_loaded(output / record["wheel"], venv)


def test_loaded_rejects_source_import_origin(prepared, tmp_path, monkeypatch):
    helper, _, output, record, _ = prepared
    module = SimpleNamespace(__file__=str(output / "source/lib/yaml/__init__.py"))
    monkeypatch.setattr(helper.importlib, "import_module", lambda name: module)
    with pytest.raises(ValueError):
        helper.verify_loaded(output / record["wheel"], tmp_path / "venv")


def test_isolated_cli_reports_only_loaded_verification(helper, monkeypatch, capsys):
    expected = {"payload": {"yaml/__init__.py": "verified"}}
    monkeypatch.setattr(
        sys, "argv", [str(SCRIPT), "--wheel", "original.whl", "--environment", "venv"]
    )
    monkeypatch.setattr(helper, "verify_loaded", lambda wheel, venv: expected)
    helper.main()
    assert json.loads(capsys.readouterr().out) == expected
