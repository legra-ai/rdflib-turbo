"""Immutable peer acquisition must not depend on advertised branch heads."""

import importlib.util
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def git(*arguments):
    return subprocess.run(
        ["git", *map(str, arguments)], check=True, capture_output=True, text=True
    ).stdout.strip()


class PinnedPeerCheckout(unittest.TestCase):
    def test_checkout_acquires_an_exact_pin_outside_remote_branches(self):
        spec = importlib.util.spec_from_file_location("ci_facade", ROOT / "scripts/ci_facade.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory(prefix="pinned-peer-") as directory:
            root = Path(directory)
            source, remote, destination = root / "source", root / "remote.git", root / "consumer"
            git("init", "--bare", remote)
            git("init", "-b", "base", source)
            git("-C", source, "config", "user.name", "Fixture")
            git("-C", source, "config", "user.email", "fixture@example.invalid")
            git("-C", source, "config", "commit.gpgsign", "false")
            (source / "input").write_text("base\n")
            git("-C", source, "add", "input")
            git("-C", source, "commit", "-m", "base")
            git("-C", source, "push", remote, "HEAD:refs/heads/base")
            git("--git-dir=" + str(remote), "symbolic-ref", "HEAD", "refs/heads/base")
            (source / "input").write_text("pinned\n")
            git("-C", source, "add", "input")
            git("-C", source, "commit", "-m", "pinned")
            revision = git("-C", source, "rev-parse", "HEAD")
            git("-C", source, "push", remote, "HEAD:refs/pins/required")

            # file:// uses the Git transport rather than copying local object files.
            module.checkout({"repository": remote.as_uri(), "revision": revision}, destination)

            self.assertEqual(git("-C", destination, "rev-parse", "HEAD"), revision)
            self.assertEqual((destination / "input").read_text(), "pinned\n")
