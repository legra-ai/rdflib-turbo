"""Workflow callers supply the collector's mandatory immutable facade inputs."""

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def job(document, name, following):
    return document.split(f"\n  {name}:\n", 1)[1].split(f"\n  {following}:\n", 1)[0]


def test_prepare_creates_one_nonrelease_evidence_artifact():
    workflow = (ROOT / ".github/workflows/ci.yml").read_text()
    prepare = job(workflow, "prepare", "sdist")
    assert prepare.count("scripts/ci_facade.py prepare") == 1
    assert "name: candidate-facade-evidence" in prepare
    assert "path: ${{ runner.temp }}/candidate-facade-evidence" in prepare
    assert "release-wheel" not in prepare


def test_every_provider_wheel_job_verifies_the_shared_artifact_before_acceptance():
    workflow = (ROOT / ".github/workflows/ci.yml").read_text()
    wheels = job(workflow, "wheels", "native-coverage")
    assert "name: candidate-facade-evidence" in wheels
    assert "scripts/ci_facade.py consume" in wheels
    call = next(line for line in wheels.splitlines() if "scripts/test_wheel.py" in line)
    assert '--shacl-wheel "$SHACL_WHEEL"' in call
    assert '--owlrl-wheel "$OWLRL_WHEEL"' in call
    assert wheels.index("scripts/ci_facade.py consume") < wheels.index("scripts/test_wheel.py")
    assert "scripts/ci_facade.py prepare" not in wheels


def test_coverage_reuses_main_artifact_and_has_an_explicit_standalone_preparation_mode():
    workflow = (ROOT / ".github/workflows/ci.yml").read_text()
    delegated = job(workflow, "native-coverage", "verify")
    assert "facade-artifact: candidate-facade-evidence" in delegated
    native = (ROOT / ".github/workflows/native-coverage.yml").read_text()
    assert "facade-artifact:" in native
    assert "if: inputs['facade-artifact'] == ''" in native
    assert "if: inputs['facade-artifact'] != ''" in native
    assert native.count("scripts/ci_facade.py prepare") == 1
    consume = next(line for line in native.splitlines() if "scripts/ci_facade.py consume" in line)
    assert "--owlrl" not in consume
    collector = next(line for line in native.splitlines() if "scripts/native_coverage.py" in line)
    for argument in (
        '--shacl-source "$SHACL_SOURCE"',
        '--shacl-wheel "$SHACL_WHEEL"',
        '--shacl-candidate-receipt "$SHACL_CANDIDATE_RECEIPT"',
        '--shacl-base-revision "$SHACL_BASE_REVISION"',
        '--owlrl-source "$OWLRL_SOURCE"',
        '--owlrl-wheel "$OWLRL_WHEEL"',
        '--owlrl-archive "$OWLRL_ARCHIVE"',
        '--owlrl-sha256 "$OWLRL_SHA256"',
    ):
        assert argument in collector
    assert "path: coverage-companion-owlrl" not in native


def test_verify_arranges_the_declared_peer_and_runs_only_the_dev_ruff_tools():
    workflow = (ROOT / ".github/workflows/ci.yml").read_text()
    verify = job(workflow, "verify", "test")
    assert "scripts/ci_facade.py consume" in verify
    assert "uv run --locked --only-group dev ruff check" in verify
    assert "uv run --locked --only-group dev ruff format --check" in verify
    assert "uv run --locked --extra test ruff" not in verify
    assert "scripts/ci_facade.py prepare" not in verify


def test_only_original_native_archives_are_selected_for_publication():
    workflow = (ROOT / ".github/workflows/ci.yml").read_text()
    publish = workflow.split("\n  publish:\n", 1)[1]
    assert "pattern: release-*" in publish
    assert "candidate-facade-evidence" not in publish
    assert "pending-packages/*" in publish


def test_downloaded_source_archive_stays_outside_the_frozen_provider_checkout():
    workflow = (ROOT / ".github/workflows/ci.yml").read_text()
    wheels = job(workflow, "wheels", "native-coverage")
    download = wheels.split("name: release-sdist", 1)[1].split("- uses:", 1)[0]
    assert "path: ${{ runner.temp }}/release-source-dist" in download
    assert (
        'python -I -B scripts/extract_source.py --archive-directory "$RUNNER_TEMP/release-source-dist" --destination source-build'
        in wheels
    )
    assert "--python 3.13" in wheels
    assert "tar -" not in wheels
    assert "path: source-dist" not in download
