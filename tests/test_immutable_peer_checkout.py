"""An immutable authority remains fetchable after its branch stops being advertised."""

import importlib.util
import subprocess
from pathlib import Path

import pytest

ROOT = Path(__file__).resolve().parents[1]


def helper():
    spec = importlib.util.spec_from_file_location("ci_facade", ROOT / "scripts/ci_facade.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def git(*arguments, cwd):
    return subprocess.run(
        ["git", *map(str, arguments)], cwd=cwd, capture_output=True, text=True, check=True
    ).stdout.strip()


@pytest.fixture
def remote(tmp_path):
    bare = tmp_path / "remote.git"
    git("init", "--bare", bare, cwd=tmp_path)
    git("config", "uploadpack.allowAnySHA1InWant", "true", cwd=bare)
    source = tmp_path / "source"
    git("init", "--initial-branch=main", source, cwd=tmp_path)
    git("config", "user.name", "Checkout test", cwd=source)
    git("config", "user.email", "checkout@example.invalid", cwd=source)
    git("config", "commit.gpgsign", "false", cwd=source)
    (source / "main.txt").write_text("advertised main\n")
    git("add", "main.txt", cwd=source)
    git("commit", "-m", "test: initial fixture", cwd=source)
    git("push", bare.as_uri(), "main", cwd=source)
    git("symbolic-ref", "HEAD", "refs/heads/main", cwd=bare)
    (source / "selected.txt").write_text("exact immutable authority\n")
    git("add", "selected.txt", cwd=source)
    git("commit", "-m", "test: retained fixture object", cwd=source)
    revision = git("rev-parse", "HEAD", cwd=source)
    git("push", bare.as_uri(), "HEAD:refs/heads/temporary", cwd=source)
    git("update-ref", "-d", "refs/heads/temporary", cwd=bare)
    return {"repository": bare.as_uri(), "revision": revision}


def test_checkout_fetches_exact_authority_missing_from_clone(remote, tmp_path):
    probe = tmp_path / "probe"
    git("clone", "--no-checkout", remote["repository"], probe, cwd=tmp_path)
    missing = subprocess.run(
        ["git", "cat-file", "-e", remote["revision"] + "^{commit}"],
        cwd=probe,
        capture_output=True,
    )
    assert missing.returncode != 0, "fixture authority unexpectedly arrived with cloned main"
    git("fetch", "--no-tags", "origin", remote["revision"], cwd=probe)
    git("cat-file", "-e", remote["revision"] + "^{commit}", cwd=probe)

    destination = tmp_path / "selected"
    helper().checkout(remote, destination)
    assert git("rev-parse", "HEAD", cwd=destination) == remote["revision"]
    assert (destination / "selected.txt").read_text() == "exact immutable authority\n"


def test_missing_exact_authority_stops_before_checkout(remote, tmp_path, monkeypatch):
    module = helper()
    original = module.run
    commands = []

    def run(arguments, *args, **kwargs):
        commands.append(list(map(str, arguments)))
        return original(arguments, *args, **kwargs)

    monkeypatch.setattr(module, "run", run)
    selected = {**remote, "revision": "f" * 40}
    with pytest.raises(subprocess.CalledProcessError):
        module.checkout(selected, tmp_path / "unavailable")
    assert any("fetch" in command for command in commands)
    assert not any("checkout" in command for command in commands)
