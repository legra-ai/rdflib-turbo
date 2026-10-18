"""Release retries must preserve the exact archives that passed verification."""

import hashlib
import importlib.util
import json
import time
import zipfile
from pathlib import Path
from urllib.error import HTTPError

import pytest


def release_module():
    spec = importlib.util.spec_from_file_location(
        "release_packages", Path(__file__).parents[1] / "scripts/release_packages.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@pytest.fixture
def release():
    return release_module()


@pytest.fixture
def packages(tmp_path):
    paths = [tmp_path / "rdflib_turbo-0.1.0.tar.gz", tmp_path / "example.whl"]
    for index, path in enumerate(paths):
        path.write_bytes(f"tested archive {index}".encode())
    return paths


def test_partial_publication_only_uploads_missing_archives(release, packages, monkeypatch):
    published = {
        "urls": [
            {
                "filename": packages[0].name,
                "digests": {"sha256": hashlib.sha256(packages[0].read_bytes()).hexdigest()},
            }
        ]
    }
    monkeypatch.setattr(release, "registry_json", lambda url: published)
    assert release.missing_packages("0.1.0", packages) == [packages[1]]
    packages[0].write_bytes(b"a different build of the same revision")
    with pytest.raises(ValueError, match="identity"):
        release.missing_packages("0.1.0", packages)


def test_unexpected_registry_file_is_not_silently_accepted(release, packages, monkeypatch):
    monkeypatch.setattr(
        release,
        "registry_json",
        lambda url: {"urls": [{"filename": "unrelated.whl", "digests": {"sha256": "bad"}}]},
    )
    with pytest.raises(ValueError, match="unexpected"):
        release.missing_packages("0.1.0", packages)


def test_verification_waits_for_all_uploaded_files_to_be_visible(release, packages, monkeypatch):
    files = [
        {"filename": path.name, "digests": {"sha256": release.digest(path)}} for path in packages
    ]
    responses = iter([None, {"urls": files[:1]}, {"urls": files}])
    monkeypatch.setattr(release, "registry_json", lambda url: next(responses))
    sleeps = []
    monkeypatch.setattr(time, "sleep", sleeps.append)
    monkeypatch.setattr(release, "inventory", lambda directory, version: packages)
    monkeypatch.setenv("RELEASE_VERSION", "0.1.0")
    monkeypatch.setattr(release.sys, "argv", ["release_packages.py", "verify"])

    release.main()

    assert sleeps == [10, 10]


def test_verification_fails_when_registry_visibility_budget_is_exhausted(
    release, packages, monkeypatch
):
    monkeypatch.setattr(release, "registry_json", lambda url: None)
    sleeps = []
    monkeypatch.setattr(release.time, "sleep", sleeps.append)

    with pytest.raises(ValueError, match=r"PyPI release incomplete.*example.whl"):
        release.verify_packages("0.1.0", packages, attempts=3, interval=10)

    assert sleeps == [10, 10]


@pytest.mark.parametrize("unexpected", [False, True])
def test_verification_never_retries_registry_identity_conflicts(
    release, packages, monkeypatch, unexpected
):
    filename = "unexpected.whl" if unexpected else packages[0].name
    monkeypatch.setattr(
        release,
        "registry_json",
        lambda url: {"urls": [{"filename": filename, "digests": {"sha256": "wrong"}}]},
    )
    sleeps = []
    monkeypatch.setattr(release.time, "sleep", sleeps.append)

    with pytest.raises(ValueError, match=r"unexpected|identity"):
        release.verify_packages("0.1.0", packages)

    assert sleeps == []


def test_verification_never_retries_registry_service_errors(release, packages, monkeypatch):
    def fail(*args, **kwargs):
        raise HTTPError("https://pypi.org", 503, "unavailable", {}, None)

    monkeypatch.setattr(release, "urlopen", fail)
    sleeps = []
    monkeypatch.setattr(release.time, "sleep", sleeps.append)

    with pytest.raises(HTTPError):
        release.verify_packages("0.1.0", packages)

    assert sleeps == []


@pytest.mark.parametrize("status", [401, 403, 429, 500])
def test_registry_errors_are_not_absent_packages(release, monkeypatch, status):
    def fail(*args, **kwargs):
        raise HTTPError("https://pypi.org", status, "error", {}, None)

    monkeypatch.setattr(release, "urlopen", fail)
    with pytest.raises(HTTPError):
        release.registry_json("https://pypi.org")


def test_bundle_roundtrip_and_revision_guard(release, packages, tmp_path):
    bundle = tmp_path / "release-packages.zip"
    release.write_bundle(bundle, "0.1.0", "a" * 40, packages)
    with pytest.raises(ValueError, match="revision"):
        release.read_bundle(bundle, "0.1.0", "b" * 40, tmp_path / "wrong")
    output = tmp_path / "restored"
    release.read_bundle(bundle, "0.1.0", "a" * 40, output)
    for package in packages:
        assert (output / package.name).read_bytes() == package.read_bytes()


@pytest.mark.parametrize("name", ["../escape.whl", "/escape.whl", "nested/escape.whl"])
def test_bundle_rejects_unsafe_names(release, tmp_path, name):
    bundle = tmp_path / "release-packages.zip"
    with zipfile.ZipFile(bundle, "w") as archive:
        archive.writestr(
            "manifest.json",
            json.dumps({"version": "0.1.0", "revision": "a" * 40, "files": {name: "bad"}}),
        )
        archive.writestr(name, b"unexpected")
    with pytest.raises(ValueError, match="filename"):
        release.read_bundle(bundle, "0.1.0", "a" * 40, tmp_path / "restored")


def test_bundle_rejects_changed_archive_bytes(release, packages, tmp_path):
    bundle = tmp_path / "release-packages.zip"
    release.write_bundle(bundle, "0.1.0", "a" * 40, packages)
    with zipfile.ZipFile(bundle) as archive:
        manifest = archive.read("manifest.json")
    with zipfile.ZipFile(bundle, "w") as archive:
        archive.writestr("manifest.json", manifest)
        for package in packages:
            archive.writestr(package.name, b"changed")
    with pytest.raises(ValueError, match="identity"):
        release.read_bundle(bundle, "0.1.0", "a" * 40, tmp_path / "restored")


def test_retry_restores_original_archives(release, packages, tmp_path, monkeypatch):
    bundle = tmp_path / "release-packages.zip"
    release.write_bundle(bundle, "0.1.0", "a" * 40, packages)
    expected = packages[0].read_bytes()
    packages[0].write_bytes(b"new build")
    monkeypatch.chdir(tmp_path)
    monkeypatch.setattr(
        release.subprocess,
        "check_output",
        lambda *a, **kw: json.dumps({"assets": [{"name": bundle.name}]}),
    )
    commands = []
    monkeypatch.setattr(release.subprocess, "run", lambda *a, **kw: commands.append((a, kw)))
    release.stage("0.1.0", "a" * 40)
    assert commands == [
        ((["gh", "release", "download", "v0.1.0", "--pattern", bundle.name],), {"check": True})
    ]
    assert (tmp_path / "release-packages" / packages[0].name).read_bytes() == expected


def test_package_license_metadata():
    import tomllib

    root = Path(__file__).parents[1]
    python = tomllib.loads((root / "pyproject.toml").read_text())["project"]
    assert python["license"] == "MIT OR Apache-2.0"
    assert "DataRoad Inc, Delaware, USA, trading as Legra" in (root / "LICENSE-MIT").read_text()
    rust = tomllib.loads((root / "Cargo.toml").read_text())["package"]
    assert python["version"] == rust["version"].replace("-alpha.", "a")
    assert python["license"] == rust["license"]


@pytest.mark.parametrize("status", [401, 403, 429, 500])
def test_draft_creation_never_treats_api_errors_as_absence(release, monkeypatch, status):
    result = release.subprocess.CompletedProcess([], 1, "", f"HTTP {status}")
    monkeypatch.setattr(release.subprocess, "run", lambda *args, **kwargs: result)
    with pytest.raises(RuntimeError, match=str(status)):
        release.ensure_draft("0.2.0a2", "a" * 40, "0.2.0-alpha.2", "owner/repo")


@pytest.mark.parametrize(
    "cargo,python,prerelease", [("0.2.0-alpha.2", "0.2.0a2", True), ("0.2.0", "0.2.0", False)]
)
def test_draft_creation_uses_exact_source_and_candidate_status(
    release, monkeypatch, cargo, python, prerelease
):
    commands = []

    def run(command, **kwargs):
        commands.append(command)
        return release.subprocess.CompletedProcess(
            [], 1 if len(commands) == 1 else 0, "", "HTTP 404"
        )

    monkeypatch.setattr(release.subprocess, "run", run)
    release.ensure_draft(python, "a" * 40, cargo, "owner/repo")
    assert commands[0] == ["gh", "api", f"repos/owner/repo/releases/tags/v{python}"]
    assert commands[1] == [
        "gh",
        "release",
        "create",
        f"v{python}",
        "--verify-tag",
        "--target",
        "a" * 40,
        "--title",
        f"v{python}",
        "--notes-file",
        "docs/release-notes.md",
        "--draft",
        *(["--prerelease"] if prerelease else []),
    ]


def test_existing_release_is_reused_without_mutation(release, monkeypatch):
    commands = []

    def run(command, **kwargs):
        commands.append(command)
        return release.subprocess.CompletedProcess([], 0, '{"draft": true}', "")

    monkeypatch.setattr(release.subprocess, "run", run)
    release.ensure_draft("0.2.0a2", "a" * 40, "0.2.0-alpha.2", "owner/repo")
    assert len(commands) == 1
