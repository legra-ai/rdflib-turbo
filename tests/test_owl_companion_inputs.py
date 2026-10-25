"""Portable OWL evidence retains exact original archives and source identities."""

import hashlib
import importlib.util
import io
import json
import tarfile
from pathlib import Path
from zipfile import ZipFile

import pytest

ROOT = Path(__file__).resolve().parents[1]
REVISION = "a" * 40
REPOSITORY = "https://github.com/legra-ai/owlrl-turbo"


def helper():
    spec = importlib.util.spec_from_file_location(
        "owl_inputs", ROOT / "scripts/owl_companion_inputs.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def sha(data):
    return hashlib.sha256(data).hexdigest()


def fixture(tmp_path):
    bundle = tmp_path / "candidate-facade-evidence"
    originals = bundle / "owl-originals"
    originals.mkdir(parents=True)
    files = {
        "pyproject.toml": b'[project]\nname="owlrl-turbo"\nversion="0.1.0a3"\n',
        "src/owlrl_turbo/__init__.py": b"original package\n",
        "PKG-INFO": b"Name: owlrl-turbo\nVersion: 0.1.0a3\n",
    }
    source = originals / "owlrl_turbo-0.1.0a3.tar.gz"
    with tarfile.open(source, "w:gz") as archive:
        for name, body in files.items():
            member = tarfile.TarInfo("owlrl_turbo-0.1.0a3/" + name)
            member.size = len(body)
            archive.addfile(member, io.BytesIO(body))
    wheel = originals / "owlrl_turbo-0.1.0a3-py3-none-any.whl"
    with ZipFile(wheel, "w") as output:
        output.writestr("owlrl_turbo/__init__.py", files["src/owlrl_turbo/__init__.py"])
        output.writestr(
            "owlrl_turbo-0.1.0a3.dist-info/METADATA", "Name: owlrl-turbo\nVersion: 0.1.0a3\n"
        )
        output.writestr(
            "owlrl_turbo-0.1.0a3.dist-info/WHEEL", "Root-Is-Purelib: true\nTag: py3-none-any\n"
        )
    record = {
        "schema": 1,
        "kind": "immutable-public-owl-test-companion",
        "publishable": False,
        "repository": REPOSITORY,
        "source": {
            "revision": REVISION,
            "files": {name: sha(body) for name, body in files.items() if name != "PKG-INFO"},
        },
        "archive": source.relative_to(bundle).as_posix(),
        "archive_sha256": sha(source.read_bytes()),
        "wheel": wheel.relative_to(bundle).as_posix(),
        "wheel_sha256": sha(wheel.read_bytes()),
        "archive_files": {name: sha(body) for name, body in files.items()},
        "package_payload": {"owlrl_turbo/__init__.py": sha(files["src/owlrl_turbo/__init__.py"])},
    }
    (bundle / "owlrl-inputs.json").write_text(json.dumps(record))
    return bundle, record


def test_relocated_originals_prove_unreleased_owl_source_and_every_package_byte(tmp_path):
    bundle, record = fixture(tmp_path)
    relocated = tmp_path / "relocated"
    bundle.rename(relocated)
    actual = helper().verify(relocated, {"repository": REPOSITORY, "revision": REVISION}, "0.1.0a3")
    assert actual["record"] == record
    assert actual["inputs"] == {
        "wheel_sha256": record["wheel_sha256"],
        "package_payload": record["package_payload"],
    }
    assert (actual["source"] / "src/owlrl_turbo/__init__.py").read_bytes() == b"original package\n"


@pytest.mark.parametrize(
    "damage",
    (
        "archive",
        "wheel",
        "revision",
        "repository",
        "git-file",
        "payload",
        "publishable",
        "locked-version",
    ),
)
def test_changed_archive_identity_payload_and_forward_version_are_rejected(tmp_path, damage):
    bundle, record = fixture(tmp_path)
    version = "0.1.0a3"
    if damage in ("archive", "wheel"):
        (bundle / record[damage]).write_bytes(b"changed original")
    elif damage == "revision":
        record["source"]["revision"] = "b" * 40
    elif damage == "repository":
        record["repository"] = "https://example.invalid/repo"
    elif damage == "git-file":
        record["source"]["files"]["src/owlrl_turbo/__init__.py"] = "b" * 64
    elif damage == "payload":
        record["package_payload"]["owlrl_turbo/__init__.py"] = "b" * 64
    elif damage == "publishable":
        record["publishable"] = True
    else:
        version = "9.9.9"
    (bundle / "owlrl-inputs.json").write_text(json.dumps(record))
    with pytest.raises(ValueError):
        helper().verify(bundle, {"repository": REPOSITORY, "revision": REVISION}, version)
