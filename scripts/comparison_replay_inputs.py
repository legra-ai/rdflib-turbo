"""Authenticate and audit original public comparison artifacts without rebuilding."""

import hashlib
import json
import platform
import re
import shutil
import stat
import subprocess
import tarfile
import tomllib
from pathlib import Path, PurePosixPath
from zipfile import ZipFile

from comparison_inputs import audit_tree, checkout, json_file

BUILD_HARNESS = "3078f8c9dd889a7b6d9cc7242734cf64df3fa2a1"
HELPERS = {
    "inference_comparison.py",
    "comparison_oracles.py",
    "comparison_attest.py",
    "comparison_ring.py",
    "comparison_capability.py",
    "comparison_small.py",
    "comparison_inputs.py",
    "comparison_results.py",
    "comparison_identity.py",
    "comparison_companion.py",
}


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def validate_origin(run_info):
    expected = {
        "event": "workflow_dispatch",
        "conclusion": "success",
        "head_sha": BUILD_HARNESS,
        "path": ".github/workflows/ci.yml",
    }
    if (
        any(run_info.get(k) != value for k, value in expected.items())
        or run_info.get("head_repository", {}).get("full_name") != "legra-ai/rdflib-turbo"
    ):
        raise ValueError("Replay requires a successful exact public build-harness dispatch")


def extract_safe(archive, output):
    output.mkdir()
    with ZipFile(archive) as bundle:
        for entry in bundle.infolist():
            name = PurePosixPath(entry.filename)
            if (
                name.is_absolute()
                or ".." in name.parts
                or "\\" in entry.filename
                or stat.S_ISLNK(entry.external_attr >> 16)
            ):
                raise ValueError("Unsafe comparison artifact path")
        for entry in bundle.infolist():
            path = output / entry.filename
            if entry.is_dir():
                path.mkdir(parents=True, exist_ok=True)
                continue
            path.parent.mkdir(parents=True, exist_ok=True)
            with bundle.open(entry) as source, path.open("wb") as destination:
                shutil.copyfileobj(source, destination)


def download(run_id, output):
    if run_id <= 0:
        raise ValueError("Expected a positive public comparison run ID")
    prefix = "repos/legra-ai/rdflib-turbo/actions"
    origin = json.loads(subprocess.check_output(["gh", "api", f"{prefix}/runs/{run_id}"]))
    validate_origin(origin)
    artifacts = json.loads(
        subprocess.check_output(["gh", "api", f"{prefix}/runs/{run_id}/artifacts"])
    )
    (artifact,) = [
        item for item in artifacts["artifacts"] if item["name"] == "performance-comparison-inputs"
    ]
    if (
        artifact["expired"]
        or re.fullmatch(r"sha256:[0-9a-f]{64}", artifact.get("digest", "")) is None
    ):
        raise ValueError("Original public artifact is expired or lacks its immutable ZIP digest")
    json_file(output / "original-run.json", origin)
    json_file(output / "original-artifact.json", artifact)
    archive = output / "input-original.zip"
    with archive.open("wb") as stream:
        subprocess.run(
            ["gh", "api", "--allow-escape-sequences", f"{prefix}/artifacts/{artifact['id']}/zip"],
            stdout=stream,
            check=True,
        )
    if digest(archive) != artifact["digest"].removeprefix("sha256:"):
        raise ValueError("Downloaded original artifact ZIP digest differs from GitHub")
    inputs = output / "inputs"
    extract_safe(archive, inputs)
    return inputs


def verify(inputs, output, base, head, harness):
    request = json.loads((inputs / "request.json").read_text())
    if request["base"] != base or request["head"] != head or request["python"] != "3.13.16":
        raise ValueError("Original comparison source/interpreter differs from requested replay")
    if request["wall_guard_seconds"] != 90 or request["rss_guard_mib"] != 1280:
        raise ValueError("Original process guards differ")
    if set(request["harness_sha256"]) != HELPERS:
        raise ValueError("Unsupported original comparison harness inventory")
    for name, expected in request["harness_sha256"].items():
        if digest(harness / name) != expected:
            raise ValueError(f"Replay changed an original benchmark method: {name}")
    toolchain = json.loads((inputs / "toolchain.json").read_text())
    if (
        not toolchain["uv"].startswith("uv 0.13.1 ")
        or not toolchain["rustc"].startswith("rustc 1.99.1 ")
        or toolchain["python"] != "3.13.16"
        or toolchain["cargo_jobs"] != 2
        or any(
            k != "CARGO_PROFILE_DEV_DEBUG" or v != "0"
            for k, v in toolchain["build_flag_environment"].items()
        )
    ):
        raise ValueError("Original build does not have the supported identical release constraints")
    receipts = {}
    for arm, commit in [("base", base), ("head", head)]:
        directory = inputs / arm
        receipt = json.loads((directory / "receipt.json").read_text())
        if receipt["commit"] != commit:
            raise ValueError("Original provider source commit mismatch")
        (archive,) = (directory / "sdist").glob("*.tar.gz")
        (wheel,) = (directory / "wheels").glob("*.whl")
        if (
            digest(archive) != receipt["archive_sha256"]
            or digest(wheel) != receipt["wheel_sha256"]
            or not wheel.name.endswith("-cp313-cp313-linux_x86_64.whl")
        ):
            raise ValueError("Original archive/wheel bytes or ABI differ")
        if "Finished `release` profile [optimized]" not in (directory / "wheel.log").read_text():
            raise ValueError("Missing actual optimized original build proof")
        source = output / (arm + "-source")
        checkout(harness.parent, commit, source, output / (arm + "-source.log"))
        unpack = output / (arm + "-archive-source")
        unpack.mkdir()
        with tarfile.open(archive) as bundle:
            bundle.extractall(unpack, filter="data")
        (extracted,) = unpack.iterdir()
        audited = audit_tree(source, extracted, receipt["source_files"])
        if audited != json.loads((directory / "source-acceptance.json").read_text())["included"]:
            raise ValueError("Restored original archive/source evidence differs")
        project = tomllib.loads((extracted / "pyproject.toml").read_text())
        if (
            project["tool"]["maturin"]["profile"] != "release"
            or not project["tool"]["maturin"]["locked"]
        ):
            raise ValueError("Original source release profile differs")
        if json.loads((directory / "start-attestation.json").read_text()) != json.loads(
            (directory / "end-attestation.json").read_text()
        ):
            raise ValueError("Original build comparison ended with changed native payloads")
        receipt.update(wheel=str(wheel), archive=str(archive))
        receipts[arm] = receipt
    owl = json.loads((inputs / "owl/receipt.json").read_text())
    (owl_archive,) = (inputs / "owl/sdist").glob("*.tar.gz")
    (owl_wheel,) = (inputs / "owl/wheels").glob("*.whl")
    if (
        digest(owl_archive) != owl["archive_sha256"]
        or digest(owl_wheel) != owl["wheel_sha256"]
        or owl["source_commit"] != "839d4877102e9c0e7b8b46eb6262a8996cd734cf"
    ):
        raise ValueError("Original public companion source/artifacts differ")
    with tarfile.open(owl_archive) as bundle:
        members = {
            entry.name.split("/", 1)[1]: entry
            for entry in bundle.getmembers()
            if entry.isfile() and "/" in entry.name
        }
        for name, expected in owl["archive_source"].items():
            if hashlib.sha256(bundle.extractfile(members[name]).read()).hexdigest() != expected:
                raise ValueError("Original companion archive source map differs")
    with ZipFile(owl_wheel) as bundle:
        payload = {
            name: hashlib.sha256(bundle.read(name)).hexdigest()
            for name in bundle.namelist()
            if name.startswith("owlrl_turbo/") and not name.endswith("/")
        }
    if payload != owl["package_payload"]:
        raise ValueError("Original companion wheel source payload differs")
    for receipt in receipts.values():
        receipt.update(owlrl_wheel=str(owl_wheel), owlrl_wheel_sha256=digest(owl_wheel))
    json_file(
        output / "input-verification.json",
        {
            "base": base,
            "head": head,
            "original_request": request,
            "original_toolchain": toolchain,
            "original_companion": owl,
            "original_archives_unchanged": True,
        },
    )
    return receipts


def require_features(required, host):
    mapping = {"fxsr": "fxsr", "sse": "sse", "sse2": "sse2"}
    if any(feature not in mapping or mapping[feature] not in host for feature in required):
        raise ValueError("Replay host does not prove every original baseline ISA feature")


def host_checks(receipts, output):
    release = dict(
        line.split("=", 1)
        for line in Path("/etc/os-release").read_text().splitlines()
        if "=" in line
    )
    if (
        platform.system() != "Linux"
        or platform.machine() != "x86_64"
        or release.get("VERSION_ID", "").strip('"') != "24.04"
        or release.get("ID") != "ubuntu"
    ):
        raise ValueError("Original native inputs require the same Ubuntu24.04 x86_64 host contract")
    cfg = subprocess.check_output(
        ["rustc", "--print", "cfg", "--target", "x86_64-unknown-linux-gnu"], text=True
    )
    required = re.findall(r'^target_feature="([^"]+)"$', cfg, re.MULTILINE)
    cpuinfo = Path("/proc/cpuinfo").read_text()
    flag_sets = [
        set(line.split(":", 1)[1].split())
        for line in cpuinfo.splitlines()
        if line.startswith("flags")
    ]
    if not flag_sets:
        raise ValueError("Missing actual host CPU feature declarations")
    flags = set.intersection(*flag_sets)
    if not required:
        raise ValueError("Missing pinned-compiler baseline feature declaration")
    require_features(required, flags)
    proof = {
        "platform": release,
        "target_cfg": cfg,
        "host_flags": sorted(flags),
        "cpu_models": sorted(
            {
                line.split(":", 1)[1].strip()
                for line in cpuinfo.splitlines()
                if line.startswith("model name")
            }
        ),
        "isa_derivation": "Original pinned-compiler baseline target and audited Cargo configs; RocksDB flags derive Cargo target features, not host-native march",
        "libc": subprocess.check_output(["getconf", "GNU_LIBC_VERSION"], text=True).strip(),
    }
    for arm, receipt in receipts.items():
        with ZipFile(receipt["wheel"]) as bundle:
            (member,) = [name for name in bundle.namelist() if name.endswith(".so")]
            library = output / (arm + "-original.so")
            with bundle.open(member) as source, library.open("wb") as destination:
                shutil.copyfileobj(source, destination)
        header = subprocess.check_output(["readelf", "-h", str(library)], text=True)
        dependencies = subprocess.check_output(["ldd", str(library)], text=True)
        notes = subprocess.check_output(["readelf", "-n", str(library)], text=True)
        if (
            "Advanced Micro Devices X86-64" not in header
            or "not found" in dependencies
            or re.search(r"x86-64-v[234]", notes)
        ):
            raise ValueError(
                "Original wheel architecture, dynamic ABI or required ISA is unsupported"
            )
        proof[arm] = {
            "elf_header": header,
            "dependencies": dependencies,
            "elf_notes": notes,
            "original_native_sha256": digest(library),
        }
    json_file(output / "host-compatibility.json", proof)
