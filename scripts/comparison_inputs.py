"""Build and retain original public source archives and native comparison wheels."""

import json
import os
import subprocess
import tarfile
import tomllib

from audit_sdist import audit
from comparison_identity import derive, verify_receipt
from comparison_model import prepare as prepare_model
from comparison_model import verify as verify_model
from comparison_source import (
    audit_tree,
    freeze,
    sha,
    verify_tree,
)
from comparison_source import (
    normalized_manifest as normalized_manifest,
)
from test_wheel import verify_metadata

PYTHON = "3.13.16"


def run(command, cwd, log, env=None):
    with log.open("w") as stream:
        subprocess.run(
            list(map(str, command)),
            cwd=cwd,
            env=env,
            stdout=stream,
            stderr=subprocess.STDOUT,
            check=True,
        )


def json_file(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def checkout(repository, commit, destination, log):
    # GitHub's public repository API must recognize the exact immutable input.
    result = subprocess.check_output(["gh", "api", f"repos/legra-ai/rdflib-turbo/commits/{commit}"])
    if json.loads(result)["sha"] != commit:
        raise ValueError("Commit is not recognized by the public repository")
    run(["git", "-C", repository, "fetch", "--no-tags", "origin", commit], repository, log)
    subprocess.run(
        ["git", "-C", str(repository), "worktree", "add", "--detach", str(destination), commit],
        check=True,
    )
    if (
        subprocess.check_output(
            ["git", "-C", str(destination), "rev-parse", "HEAD"], text=True
        ).strip()
        != commit
    ):
        raise ValueError("Checkout revision changed")


def build(source, output, commit):
    output.mkdir()
    start = freeze(source)
    json_file(output / "git-source.json", {"commit": commit, "files": start})
    run(
        [
            "uv",
            "build",
            "--sdist",
            "--out-dir",
            output / "sdist",
            "-C",
            "maturin.build-args=--locked",
        ],
        source,
        output / "sdist.log",
    )
    (archive,) = (output / "sdist").glob("*.tar.gz")
    archive_sha = sha(archive)
    audit(archive)
    unpack = output / "extracted"
    unpack.mkdir()
    with tarfile.open(archive) as bundle:
        bundle.extractall(unpack, filter="data")
    (extracted,) = unpack.iterdir()
    included = audit_tree(source, extracted, start)
    json_file(
        output / "source-acceptance.json",
        {"commit": commit, "archive_sha256": archive_sha, "included": included},
    )
    project = tomllib.loads((extracted / "pyproject.toml").read_text())
    if (
        project["tool"]["maturin"]["profile"] != "release"
        or not project["tool"]["maturin"]["locked"]
    ):
        raise ValueError("Comparison requires the original locked release profile")
    features = project["tool"]["maturin"]["features"]
    with (output / "cargo-metadata.json").open("w") as stream:
        subprocess.run(
            [
                "cargo",
                "metadata",
                "--locked",
                "--format-version",
                "1",
                "--filter-platform",
                "x86_64-unknown-linux-gnu",
                "--features",
                ",".join(features),
            ],
            cwd=extracted,
            stdout=stream,
            check=True,
        )
    metadata_path = output / "cargo-metadata.json"
    metadata = json.loads(metadata_path.read_text())
    identity = derive(extracted, metadata)
    payload = verify_tree(extracted, archive, archive_sha)
    model = prepare_model(extracted, metadata, output / "model")
    json_file(output / "model-source.json", model)
    json_file(output / "expected-identity.json", identity)
    env = {**os.environ, "CARGO_BUILD_JOBS": "2", "CARGO_TARGET_DIR": str(output / "target")}
    # These native Linux inputs are comparison artifacts, never release inventory.
    run(
        [
            "uv",
            "build",
            archive,
            "--wheel",
            "--python",
            PYTHON,
            "--out-dir",
            output / "wheels",
            "-C",
            "maturin.build-args=--locked --compatibility linux",
        ],
        output,
        output / "wheel.log",
        env,
    )
    (wheel,) = (output / "wheels").glob("*.whl")
    verify_metadata(wheel)
    if freeze(source) != start or sha(archive) != archive_sha:
        raise ValueError("Build mutated original source or archive")
    verify_model(extracted, metadata, model)
    receipt = {
        "commit": commit,
        "source_checkout": str(source),
        "archive_source": str(extracted),
        "archive_files": payload,
        "cargo_metadata": str(metadata_path),
        "cargo_metadata_sha256": sha(metadata_path),
        "model_source": model,
        "wheel": str(wheel),
        "wheel_sha256": sha(wheel),
        "archive": str(archive),
        "archive_sha256": archive_sha,
        "expected_identity": identity,
        "source_files": start,
    }
    verify_receipt(receipt)
    return receipt


def project(path, output, wheels=(), reference=False):
    run(
        ["uv", "init", "--bare", "--no-workspace", "--vcs", "none", "--python", PYTHON, path],
        output,
        output / (path.name + "-init.log"),
    )
    run(
        ["uv", "python", "pin", "--project", path, PYTHON],
        output,
        output / (path.name + "-pin.log"),
    )
    requirements = ["rdflib==7.6.0", *map(str, wheels)]
    if reference:
        requirements.append("owlrl==7.6.2")
    run(
        ["uv", "add", "--project", path, "--no-sync", *requirements],
        output,
        output / (path.name + "-add.log"),
    )
    run(["uv", "sync", "--project", path, "--locked"], output, output / (path.name + "-sync.log"))
