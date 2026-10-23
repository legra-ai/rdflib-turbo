"""Opt-in paired public builds and bounded cold inference measurements.

Correctness and provenance are hard gates. Timings never control release gates.
Original input archives and receipts are retained separately from release inputs.
"""

import argparse
import ctypes
import ctypes.util
import datetime
import json
import os
import re
import signal
import statistics
import subprocess
import time
from pathlib import Path

from comparison_inputs import (
    audit_tree as audit_tree,
)
from comparison_inputs import (
    build,
    checkout,
    freeze,
    json_file,
    owl_input,
    project,
    run,
    sha,
)
from comparison_inputs import (
    normalized_manifest as normalized_manifest,
)

ARMS = ("base", "head", "head", "base", "head", "base", "base", "head")
HERE = Path(__file__).resolve().parent


def validate_refs(base, head):
    if base == head or any(re.fullmatch(r"[0-9a-f]{40}", value) is None for value in (base, head)):
        raise ValueError("Supply two distinct immutable lowercase full Git commit SHAs")


def timing_summary(values):
    result = {"n": len(values), "values": values, "status": "inconclusive"}
    if not values:
        return result
    result["median"] = statistics.median(values)
    if len(values) > 1:
        result["sample_cv_percent"] = 100 * statistics.stdev(values) / statistics.mean(values)
    if len(values) == 4 and result.get("sample_cv_percent", 100) <= 5:
        result["status"] = "complete"
    return result


def require_equal_witnesses(values):
    if values and any(value != values[0] for value in values):
        raise ValueError("Complete output witness mismatch")


def command(runtime, script, *arguments):
    return [
        "uv",
        "run",
        "--project",
        str(runtime),
        "--locked",
        "--no-sync",
        "python",
        "-I",
        "-B",
        str(HERE / script),
        *map(str, arguments),
    ]


def stop_child(child):
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass  # Owned child can exit between the poll and signal.
    try:
        child.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(child.pid, signal.SIGKILL)
        child.wait()


def guarded(command_line, output, index):
    log = output / f"worker-{index:03d}.log"
    start = time.monotonic()
    reason = None
    peak = 0
    samples = []
    with log.open("w") as stream:
        child = subprocess.Popen(
            command_line,
            cwd=output,
            stdout=stream,
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )
        try:
            while child.poll() is None:
                table = subprocess.check_output(["ps", "-axo", "pid=,ppid=,rss="], text=True)
                entries = [tuple(map(int, line.split())) for line in table.splitlines()]
                ids = {child.pid}
                # Fixed-point discovery includes all owned descendants.
                while True:
                    before = len(ids)
                    ids.update(pid for pid, parent, _ in entries if parent in ids)
                    if before == len(ids):
                        break
                rss = sum(kib * 1024 for pid, _, kib in entries if pid in ids)
                peak = max(peak, rss)
                elapsed = time.monotonic() - start
                samples.append([elapsed, rss])
                if rss > 1280 * 1024 * 1024:
                    reason = "1280MiB owned-child RSS guard"
                elif elapsed > 90:
                    reason = "90second wall guard"
                if reason:
                    break
                time.sleep(0.25)
        finally:
            if child.poll() is None:
                stop_child(child)
        exit_code = child.wait()
    records = [json.loads(line) for line in log.read_text().splitlines() if line.startswith("{")]
    if exit_code and not reason:
        raise ValueError(f"Worker failed independently of timing guard: {log}")
    return {
        "command": command_line,
        "exit_code": exit_code,
        "guard": reason,
        "elapsed_seconds": time.monotonic() - start,
        "sampled_peak_bytes": peak,
        "rss_samples": samples,
        "records": records,
        "log": log.name,
    }


def preflight(output):
    versions = {}
    for name, args in {
        "uv": ["uv", "--version"],
        "rustc": ["rustc", "--version"],
        "clang": ["clang", "--version"],
    }.items():
        versions[name] = subprocess.check_output(args, text=True).strip()
    if not versions["uv"].startswith("uv 0.13.1 ") or not versions["rustc"].startswith(
        "rustc 1.99.1 "
    ):
        raise ValueError("Comparison toolchain differs from the pinned CI contract")
    # A clang executable or libclang-cpp does not prove the bindgen C API exists.
    libraries = sorted(Path("/usr/lib").glob("llvm*/lib/libclang.so*"))
    located = ctypes.util.find_library("clang")
    if located:
        libraries.append(Path(located))
    loaded = None
    for library in libraries:
        try:
            candidate = ctypes.CDLL(str(library))
            candidate.clang_createIndex
            loaded = str(library)
            break
        except (OSError, AttributeError):
            continue  # This candidate cannot supply bindgen's required C API.
    if loaded is None:
        raise ValueError("No actual libclang C API shared library available for RocksDB bindgen")
    overrides = {
        k: v
        for k, v in os.environ.items()
        if k.startswith(("CARGO_PROFILE_", "RUSTFLAGS", "CARGO_ENCODED_RUSTFLAGS"))
    }
    # Preserve the existing setup-rust-toolchain action's warning-denial flag.
    allowed = {"RUSTFLAGS": "-D warnings"} if overrides.get("RUSTFLAGS") == "-D warnings" else {}
    if overrides != allowed:
        raise ValueError(f"Unexpected build-profile overrides: {sorted(overrides)}")
    json_file(
        output / "toolchain.json",
        {
            **versions,
            "libclang_loaded": loaded,
            "build_flag_environment": overrides,
            "python": "3.13.16",
            "cargo_jobs": 2,
        },
    )


def run_comparison(args):
    validate_refs(args.base, args.head)
    output = args.output.resolve()
    output.mkdir(exist_ok=False)
    preflight(output)
    repository = HERE.parent
    receipts = {}
    runtimes = {}
    source_freezes = {}
    owl = owl_input(output)
    for arm, revision in [("base", args.base), ("head", args.head)]:
        source = output / (arm + "-source")
        checkout(repository, revision, source, output / (arm + "-fetch.log"))
        receipt = build(source, output / arm, revision)
        receipt.update(owlrl_wheel=str(owl), owlrl_wheel_sha256=sha(owl))
        json_file(output / arm / "receipt.json", receipt)
        receipts[arm] = receipt
        source_freezes[arm] = freeze(source)
        runtime = output / (arm + "-runtime")
        project(runtime, output, [Path(receipt["wheel"]), owl])
        runtimes[arm] = runtime
    # Lockfiles, compiler profile and native build configuration must match;
    # different algorithms are the comparison, different build settings are not.
    for name in (
        "Cargo.lock",
        "uv.lock",
        "Cargo.toml",
        "pyproject.toml",
        "rust-toolchain.toml",
        ".cargo/config.toml",
    ):
        if sha(output / "base-source" / name) != sha(output / "head-source" / name):
            raise ValueError(f"Paired build settings/dependencies differ: {name}")
    reference = output / "oracle-runtime"
    project(reference, output, reference=True)
    run(
        command(reference, "comparison_oracles.py", "--output", output / "oracles"),
        output,
        output / "oracle.log",
    )
    fixtures = output / "oracles/fixtures.json"
    entries = json.loads(fixtures.read_text())["entries"]
    project_hashes = {
        arm: {
            name: sha(runtime / name) for name in ("pyproject.toml", "uv.lock", ".python-version")
        }
        for arm, runtime in runtimes.items()
    }
    attestations = {}
    for arm, runtime in runtimes.items():
        data = subprocess.check_output(
            command(runtime, "comparison_attest.py", output / arm / "receipt.json"), cwd=output
        )
        attestations[arm] = json.loads(data)
        json_file(output / arm / "start-attestation.json", attestations[arm])
        for index, entry in enumerate(entries):
            result = guarded(
                command(
                    runtime,
                    "comparison_small.py",
                    "--fixtures",
                    fixtures,
                    "--case",
                    entry["case"],
                    "--mode",
                    entry["mode"],
                ),
                output / arm,
                index,
            )
            if result["guard"] or not any(
                x.get("full_small_graph_isomorphic") for x in result["records"]
            ):
                raise ValueError("Mandatory full small graph oracle did not complete")
    # Every build and correctness worker is complete before this timing phase.
    results = []
    groups = [
        (
            f"ring8-{size}",
            ["comparison_ring.py", "--implementation", "turbo", "--input-count", size],
        )
        for size in (25000, 100000)
    ]
    groups.extend(
        (case, ["comparison_small.py", "--fixtures", fixtures, "--case", case])
        for case in (
            "isolated-selfs",
            "closed-pairs",
            "closed-clique16",
            "sparse-chain16",
            "half-circulant16",
        )
    )
    for case, worker in groups:
        witnesses = []
        for arm in ARMS:
            print(
                json.dumps(
                    {
                        "event": "start",
                        "case": case,
                        "arm": arm,
                        "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                    }
                ),
                flush=True,
            )
            result = guarded(command(runtimes[arm], *worker), output, len(results))
            result.update(case=case, arm=arm)
            results.append(result)
            json_file(output / "raw-results.json", results)
            if result["guard"]:
                continue
            parsed = [x for x in result["records"] if x["stage"] == "parsed"]
            (audited,) = [x for x in result["records"] if x["stage"] == "audited"]
            if parsed:
                witnesses.append(
                    {
                        **audited,
                        "input_count": parsed[0]["input_count"],
                        "input_sha256": parsed[0]["input_sha256"],
                    }
                )
            elif not audited.get("full_small_graph_isomorphic"):
                raise ValueError("Missing small full-graph proof")
            require_equal_witnesses(witnesses)
    for arm, runtime in runtimes.items():
        data = json.loads(
            subprocess.check_output(
                command(runtime, "comparison_attest.py", output / arm / "receipt.json"), cwd=output
            )
        )
        if data != attestations[arm] or freeze(output / (arm + "-source")) != source_freezes[arm]:
            raise ValueError("Source or installed native payload changed during comparison")
        if project_hashes[arm] != {name: sha(runtime / name) for name in project_hashes[arm]}:
            raise ValueError("Runtime project or lockfile changed")
        for key in ("wheel", "archive", "owlrl_wheel"):
            if sha(Path(receipts[arm][key])) != receipts[arm][key + "_sha256"]:
                raise ValueError("Original comparison archive changed")
        json_file(output / arm / "end-attestation.json", data)
    summary = {}
    for case, _ in groups:
        arms = {}
        for arm in ("base", "head"):
            selected = [x for x in results if x["case"] == case and x["arm"] == arm]
            times = [
                next(r for r in x["records"] if r["stage"] == "inferred")["inference_cpu_ns"] / 1e9
                for x in selected
                if not x["guard"]
            ]
            arms[arm] = timing_summary(times)
        complete = all(value["status"] == "complete" for value in arms.values())
        summary[case] = {"arms": arms, "status": "complete" if complete else "inconclusive"}
        if complete:
            summary[case]["head_over_base_cpu_median"] = (
                arms["head"]["median"] / arms["base"]["median"]
            )
    json_file(
        output / "summary.json",
        {
            "cases": summary,
            "all_oracles_and_provenance_passed": True,
            "limits": "Timing is informational; high CV/incomplete cases are inconclusive. Streaming large witnesses are not full stored graph proofs. No extrapolation to other workloads.",
        },
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--base", required=True)
    parser.add_argument("--head", required=True)
    parser.add_argument("--output", type=Path, required=True)
    run_comparison(parser.parse_args())
