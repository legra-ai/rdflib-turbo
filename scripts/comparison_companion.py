"""Build a test companion from immutable public source, never relabel a release."""

import json
import subprocess
import tarfile
from pathlib import Path

from audit_sdist import audit
from comparison_inputs import PYTHON, freeze, json_file, run, sha
from pure_companion import verify_payload

OWL_REVISION = "839d4877102e9c0e7b8b46eb6262a8996cd734cf"


def verify_originals(output):
    receipt = json.loads((output / "owl/receipt.json").read_text())
    for key in ("archive", "wheel"):
        if sha(Path(receipt[key])) != receipt[key + "_sha256"]:
            raise ValueError(f"Original public companion {key} changed")
    if freeze(output / "owl-source") != receipt["git_source"]:
        raise ValueError("Original public companion source changed")


def owl_input(output):
    result = json.loads(
        subprocess.check_output(["gh", "api", f"repos/legra-ai/owlrl-turbo/commits/{OWL_REVISION}"])
    )
    if result["sha"] != OWL_REVISION:
        raise ValueError("Public companion commit differs from requested immutable source")
    source = output / "owl-source"
    run(
        ["git", "clone", "--no-checkout", "https://github.com/legra-ai/owlrl-turbo", source],
        output,
        output / "owl-clone.log",
    )
    run(
        ["git", "-C", source, "checkout", "--detach", OWL_REVISION],
        output,
        output / "owl-checkout.log",
    )
    actual = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    if actual != OWL_REVISION:
        raise ValueError("Companion checkout revision changed")
    start = freeze(source)
    destination = output / "owl"
    destination.mkdir()
    run(
        ["uv", "build", "--sdist", "--out-dir", destination / "sdist"],
        source,
        destination / "sdist.log",
    )
    (archive,) = (destination / "sdist").glob("*.tar.gz")
    archive_sha = sha(archive)
    audit(archive)
    unpack = destination / "extracted"
    unpack.mkdir()
    with tarfile.open(archive) as bundle:
        bundle.extractall(unpack, filter="data")
    (extracted,) = unpack.iterdir()
    included = {}
    for name, digest in start.items():
        path = extracted / name
        if path.is_file():
            if sha(path) != digest:
                raise ValueError(f"Companion archive changed public source: {name}")
            included[name] = digest
        elif name.startswith("src/") or name in ("pyproject.toml", "uv.lock"):
            raise ValueError(f"Companion archive is missing source: {name}")
    run(
        [
            "uv",
            "build",
            archive,
            "--wheel",
            "--python",
            PYTHON,
            "--out-dir",
            destination / "wheels",
        ],
        output,
        destination / "wheel.log",
    )
    (wheel,) = (destination / "wheels").glob("*.whl")
    payload = verify_payload(source, wheel)
    if freeze(source) != start or sha(archive) != archive_sha:
        raise ValueError("Companion build mutated original public inputs")
    json_file(
        destination / "receipt.json",
        {
            "source_commit": OWL_REVISION,
            "source_repository": "https://github.com/legra-ai/owlrl-turbo",
            "contract": "Unreleased public-source test companion; not the PyPI 0.1.0a3 runtime",
            "archive": str(archive),
            "archive_sha256": archive_sha,
            "wheel": str(wheel),
            "wheel_sha256": sha(wheel),
            "git_source": start,
            "archive_source": included,
            "package_payload": payload,
        },
    )
    return wheel
