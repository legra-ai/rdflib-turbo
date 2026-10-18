"""Explicit-import rdfpandas migration: complete roundtrip observations and timings."""

import argparse
import gc
import hashlib
import importlib.util
import json
import platform
import resource
import statistics
import subprocess
import sys
import time
from importlib import metadata
from pathlib import Path


def worker(args):
    import pandas as pd

    source = Path(args.baseline) / "rdfpandas/graph.py"
    if args.kind == "baseline":
        spec = importlib.util.spec_from_file_location("baseline_rdfpandas_graph", source)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
    else:
        import rdflib_turbo
        import rdfpandas.graph as module

        assert module.Graph is rdflib_turbo.Graph
    if args.kind == "baseline" and "rdflib_turbo" in sys.modules:
        raise RuntimeError("Baseline imported native Turbo module")
    frame = pd.DataFrame(
        {
            "https://example.org/name{Literal}": [f"name-{i}" for i in range(args.records)],
            "https://example.org/value{Literal}": [f"value-{i % 17}" for i in range(args.records)],
            "https://example.org/link{URIRef}": [
                f"https://example.org/object/{i % 23}" for i in range(args.records)
            ],
        },
        index=[f"https://example.org/subject/{i}" for i in range(args.records)],
    )
    samples, observations = [], []
    for index in range(args.warmups + args.samples):
        gc.collect()
        start = time.perf_counter_ns()
        graph = module.to_graph(frame)
        output = module.to_dataframe(graph)
        elapsed = (time.perf_counter_ns() - start) / 1e6
        rss = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss / (
            1024 * 1024 if sys.platform == "darwin" else 1024
        )
        # Full triples and full sorted dataframe are observed after timer/RSS capture.
        observation = {
            "triples": sorted([[term.n3() for term in triple] for triple in graph]),
            "dataframe": json.loads(output.sort_index().sort_index(axis=1).to_json(orient="split")),
        }
        if index >= args.warmups:
            samples.append({"milliseconds": elapsed, "peak_rss_mib": rss})
            observations.append(observation)
        del graph, output
    return {
        "kind": args.kind,
        "records": args.records,
        "samples": samples,
        "observations": observations,
        "python": sys.version,
        "platform": platform.platform(),
        "graph_class": f"{module.Graph.__module__}.{module.Graph.__name__}",
        "rdfpandas_source": str(Path(module.__file__).resolve()),
        "rdfpandas_source_sha256": hashlib.sha256(Path(module.__file__).read_bytes()).hexdigest(),
        "environment_lock_sha256": hashlib.sha256(
            (Path(sys.prefix).parent / "uv.lock").read_bytes()
        ).hexdigest(),
        "versions": {
            name: metadata.version(name)
            for name in ("pandas", "numpy", "rdflib", "rdflib-turbo", "rdfpandas")
        },
        "native_extension_sha256": (
            hashlib.sha256(
                Path(sys.modules["rdflib_turbo._native"].__file__).read_bytes()
            ).hexdigest()
            if args.kind == "candidate"
            else None
        ),
        "native_module_loaded": "rdflib_turbo" in sys.modules,
    }


def call(arguments, launcher_root):
    sys.path.insert(0, str(launcher_root))
    from upstream_support.process_launch import Popen

    with Popen(
        [sys.executable, str(Path(__file__).resolve()), "--worker", *arguments],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    ) as process:
        try:
            output, errors = process.communicate(timeout=180)
        except subprocess.TimeoutExpired:
            process.kill()
            process.communicate()
            raise RuntimeError("Roundtrip worker timed out") from None
        if process.returncode:
            raise RuntimeError(f"Worker exited {process.returncode}: {errors}")
        return json.loads(output)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--worker", action="store_true")
    parser.add_argument("--kind", choices=["baseline", "candidate"])
    parser.add_argument("--records", type=int)
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--warmups", type=int, default=1)
    parser.add_argument("--baseline", default="/tmp/turbo-parity-downstream-20261017/rdfpandas")
    parser.add_argument(
        "--launcher-root",
        type=Path,
        required=True,
        help="pyshacl-turbo tests directory containing audited safe process launcher",
    )
    parser.add_argument("--output", type=Path, default=Path("roundtrip.json"))
    args = parser.parse_args()
    if args.worker:
        print(json.dumps(worker(args), sort_keys=True))
        return
    result = {
        "schema": 1,
        "measurement": "Explicit Graph import migration, not unchanged imports",
        "harness_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "cases": [],
    }
    for records in (100, 1000):
        case = {"records": records, "runs": {}}
        for kind in ("baseline", "candidate"):
            arguments = [
                "--kind",
                kind,
                "--records",
                str(records),
                "--baseline",
                args.baseline,
                "--launcher-root",
                str(args.launcher_root),
            ]
            timing = call(arguments, args.launcher_root)
            memory = [
                call([*arguments, "--samples", "1", "--warmups", "0"], args.launcher_root)
                for _ in range(3)
            ]
            observations = timing["observations"] + [r["observations"][0] for r in memory]
            if any(o != observations[0] for o in observations):
                raise RuntimeError(f"Unstable full observations: {kind}/{records}")
            case["runs"][kind] = {
                "timing": timing,
                "cold_memory": memory,
                "median_ms": statistics.median(s["milliseconds"] for s in timing["samples"]),
                "median_rss_mib": statistics.median(
                    r["samples"][0]["peak_rss_mib"] for r in memory
                ),
            }
        case["full_observations_equal"] = (
            case["runs"]["baseline"]["timing"]["observations"][0]
            == case["runs"]["candidate"]["timing"]["observations"][0]
        )
        if not case["full_observations_equal"]:
            raise RuntimeError(f"Complete roundtrip mismatch: {records}")
        result["cases"].append(case)
        print(
            json.dumps(
                {
                    "records": records,
                    **{
                        k: {n: r[n] for n in ("median_ms", "median_rss_mib")}
                        for k, r in case["runs"].items()
                    },
                }
            )
        )
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, sort_keys=True) + "\n")


if __name__ == "__main__":
    main()
