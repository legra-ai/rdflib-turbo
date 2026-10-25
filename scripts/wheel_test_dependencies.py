"""Fetch the frozen host Cargo graph before offline wheel-acceptance identity probes."""

import importlib.util
import subprocess
from pathlib import Path


def run(arguments, *, capture=False):
    result = subprocess.run(
        list(map(str, arguments)),
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )
    return result.stdout if capture else None


def freeze_source(root):
    spec = importlib.util.spec_from_file_location(
        "candidate_source", Path(__file__).with_name("build_facade_candidate.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    revision = run(["git", "-C", root, "rev-parse", "HEAD"], capture=True).strip()
    return module.freeze(root, revision, "rdflib-turbo", run)


def prepare(root):
    root = Path(root).resolve(strict=True)
    before = freeze_source(root)
    try:
        # Cargo inherits the caller's offline configuration; failure is never retried.
        subprocess.run(
            ["cargo", "fetch", "--locked", "--manifest-path", str(root / "Cargo.toml")],
            cwd=root,
            check=True,
        )
    finally:
        if freeze_source(root) != before:
            raise RuntimeError(
                "Frozen wheel acceptance source or lock changed during dependency fetch"
            )
    return before
