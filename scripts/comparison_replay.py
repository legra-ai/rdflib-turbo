"""Bounded 25k replay of original public wheels; no native build or sample removal."""

import argparse
import datetime
import json
import statistics
import tomllib
from pathlib import Path

from comparison_inputs import json_file, run
from comparison_replay_inputs import digest, download, host_checks, verify
from inference_comparison import (
    ARMS,
    HERE,
    command,
    guarded,
    preflight,
    require_equal_witnesses,
    validate_refs,
)


def measurement(values):
    result = {"n": len(values), "values": values, "status": "inconclusive"}
    if not values:
        return result
    result.update(median=statistics.median(values), minimum=min(values), maximum=max(values))
    if len(values) > 1:
        result["sample_cv_percent"] = 100 * statistics.stdev(values) / statistics.mean(values)
    if len(values) == 8 and result.get("sample_cv_percent", 100) <= 5:
        result["status"] = "complete"
    return result


def registry_packages(lock):
    return {
        package["name"]: package for package in lock["package"] if "registry" in package["source"]
    }


def runtime_project(inputs, arm, receipt, output):
    runtime = output / (arm + "-runtime")
    original = registry_packages(tomllib.loads((inputs / (arm + "-runtime/uv.lock")).read_text()))
    requirements = [f"{name}=={package['version']}" for name, package in original.items()]
    commands = [
        ["uv", "init", "--bare", "--no-workspace", "--vcs", "none", "--python", "3.13.16", runtime],
        ["uv", "python", "pin", "--project", runtime, "3.13.16"],
        [
            "uv",
            "add",
            "--project",
            runtime,
            "--no-sync",
            *requirements,
            receipt["wheel"],
            receipt["owlrl_wheel"],
        ],
        ["uv", "sync", "--project", runtime, "--locked"],
    ]
    for index, invocation in enumerate(commands):
        run(invocation, output, output / f"{arm}-project-{index}.log")
    current = registry_packages(tomllib.loads((runtime / "uv.lock").read_text()))
    if current != original:
        raise ValueError(
            "Replay changed original locked registry package versions or archive hashes"
        )
    return runtime


def fixtures(inputs, output):
    path = inputs / "oracles/fixtures.json"
    data = json.loads(path.read_text())
    if len(data["entries"]) != 30:
        raise ValueError("Expected all30 original small full-graph oracles")
    for entry in data["entries"]:
        for key in ("input", "expected"):
            restored = inputs / "oracles" / Path(entry[key]).name
            if digest(restored) != entry[key + "_sha256"]:
                raise ValueError("Original oracle/input changed")
            entry[key] = str(restored)
    result = output / "replay-fixtures.json"
    json_file(result, data)
    json_file(
        output / "oracle-provenance.json",
        {
            "original_manifest_sha256": digest(path),
            "restored_manifest_sha256": digest(result),
            "only_file_locations_rewritten": True,
        },
    )
    return result, data["entries"]


def attest(runtime, receipt, output, name):
    log = output / (name + ".json")
    run(command(runtime, "comparison_attest.py", receipt), output, log)
    return json.loads(log.read_text())


def replay(args):
    validate_refs(args.base, args.head)
    output = args.output.resolve()
    output.mkdir(exist_ok=False)
    json_file(
        output / "request.json",
        {
            "input_run": args.input_run,
            "base": args.base,
            "head": args.head,
            "workload": "ring8-25000",
            "cold_repetitions_per_arm": 8,
            "order": list(ARMS) * 2,
            "wall_guard_seconds": 90,
            "rss_guard_mib": 1280,
            "replay_coordinator_sha256": digest(Path(__file__)),
        },
    )
    preflight(output)
    inputs = download(args.input_run, output)
    receipts = verify(inputs, output, args.base, args.head, HERE)
    host_checks(receipts, output)
    original_paths = [
        output / "input-original.zip",
        *inputs.glob("*/sdist/*.tar.gz"),
        *inputs.glob("*/wheels/*.whl"),
    ]
    if len(original_paths) != 7:
        raise ValueError("Expected exactly three original source archives and three wheels")
    original_hashes = {path: digest(path) for path in original_paths}
    fixture, entries = fixtures(inputs, output)
    runtimes = {}
    before = {}
    project_hashes = {}
    for arm, receipt in receipts.items():
        path = output / (arm + "-receipt.json")
        json_file(path, receipt)
        runtime = runtime_project(inputs, arm, receipt, output)
        runtimes[arm] = runtime
        project_hashes[arm] = {
            name: digest(runtime / name)
            for name in ("pyproject.toml", "uv.lock", ".python-version")
        }
        before[arm] = attest(runtime, path, output, arm + "-start-attestation")
        run(
            command(runtime, "comparison_capability.py"), output, output / (arm + "-capability.log")
        )
        oracle_output = output / (arm + "-oracles")
        oracle_output.mkdir()
        for index, entry in enumerate(entries):
            row = guarded(
                command(
                    runtime,
                    "comparison_small.py",
                    "--fixtures",
                    fixture,
                    "--case",
                    entry["case"],
                    "--mode",
                    entry["mode"],
                ),
                oracle_output,
                index,
            )
            if row["guard"] or not any(
                r.get("full_small_graph_isomorphic") for r in row["records"]
            ):
                raise ValueError("Mandatory original small graph oracle did not complete")
    original = [
        row
        for row in json.loads((inputs / "raw-results.json").read_text())
        if row["case"] == "ring8-25000"
    ]
    original_witnesses = [
        next(r for r in row["records"] if r["stage"] == "audited") for row in original
    ]
    require_equal_witnesses(original_witnesses)
    if len(original_witnesses) != 8:
        raise ValueError("Original25k comparison lacks every complete output witness")
    results = []
    witnesses = original_witnesses[:1]
    for index, arm in enumerate(list(ARMS) * 2):
        print(
            json.dumps(
                {
                    "event": "start",
                    "index": index,
                    "arm": arm,
                    "utc": datetime.datetime.now(datetime.timezone.utc).isoformat(),
                }
            ),
            flush=True,
        )
        row = guarded(
            command(
                runtimes[arm],
                "comparison_ring.py",
                "--implementation",
                "turbo",
                "--input-count",
                25000,
            ),
            output,
            index,
        )
        row.update(arm=arm, index=index)
        results.append(row)
        json_file(output / "raw-results.json", results)
        if not row["guard"]:
            (audited,) = [r for r in row["records"] if r["stage"] == "audited"]
            witnesses.append(audited)
            require_equal_witnesses(witnesses)
        print(
            json.dumps(
                {
                    "event": "complete",
                    "index": index,
                    "arm": arm,
                    "guard": row["guard"],
                    "records": row["records"],
                }
            ),
            flush=True,
        )
    for arm, runtime in runtimes.items():
        if (
            attest(runtime, output / (arm + "-receipt.json"), output, arm + "-end-attestation")
            != before[arm]
        ):
            raise ValueError("Replay changed installed native payloads")
        if project_hashes[arm] != {name: digest(runtime / name) for name in project_hashes[arm]}:
            raise ValueError("Replay changed runtime manifests/lockfiles")
        for key in ("archive", "wheel", "owlrl_wheel"):
            if digest(Path(receipts[arm][key])) != receipts[arm][key + "_sha256"]:
                raise ValueError("Replay changed an original build input")
    if original_hashes != {path: digest(path) for path in original_paths}:
        raise ValueError("Replay changed an original ZIP, source archive or wheel")
    json_file(
        output / "end-original-inputs.json",
        {str(path.relative_to(output)): value for path, value in original_hashes.items()},
    )
    arms = {}
    for arm in ("base", "head"):
        inferred = [
            next(r for r in row["records"] if r["stage"] == "inferred")
            for row in results
            if row["arm"] == arm and not row["guard"]
        ]
        arms[arm] = {
            "cpu_seconds": measurement([r["inference_cpu_ns"] / 1e9 for r in inferred]),
            "wall_seconds": measurement([r["inference_wall_ns"] / 1e9 for r in inferred]),
            "peak_before_export_bytes": measurement(
                [r["peak_before_export_bytes"] for r in inferred]
            ),
        }
    complete = all(arm["cpu_seconds"]["status"] == "complete" for arm in arms.values())
    summary = {
        "arms": arms,
        "status": "complete" if complete else "inconclusive",
        "all60_original_small_oracles_passed": True,
        "original_inputs_unchanged": True,
        "all_samples_retained": True,
        "limits": "Informational timing only; no ratio for missing/high-CV arms. Streaming large witnesses are not complete stored graph proofs.",
    }
    pairs = []
    for offset in range(0, len(results), 2):
        pair = results[offset : offset + 2]
        observation = {
            "indices": [row["index"] for row in pair],
            "complete": not any(row["guard"] for row in pair),
        }
        if observation["complete"]:
            values = {
                row["arm"]: next(r for r in row["records"] if r["stage"] == "inferred")
                for row in pair
            }
            observation["head_minus_base_cpu_seconds"] = (
                values["head"]["inference_cpu_ns"] - values["base"]["inference_cpu_ns"]
            ) / 1e9
            observation["head_minus_base_wall_seconds"] = (
                values["head"]["inference_wall_ns"] - values["base"]["inference_wall_ns"]
            ) / 1e9
        pairs.append(observation)
    summary["all_adjacent_pair_observations"] = pairs
    if complete:
        summary["head_over_base_cpu_median"] = (
            arms["head"]["cpu_seconds"]["median"] / arms["base"]["cpu_seconds"]["median"]
        )
    json_file(output / "summary.json", summary)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--input-run", type=int, required=True)
    parser.add_argument("--base", required=True)
    parser.add_argument("--head", required=True)
    parser.add_argument("--output", type=Path, required=True)
    replay(parser.parse_args())
