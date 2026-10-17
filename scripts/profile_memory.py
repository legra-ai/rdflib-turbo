"""Measure native Rust allocation peaks by phase in an isolated release test binary.

Run through uv with the selected Python interpreter. No profiling code is enabled
in ordinary tests or wheels. Counts are requested Rust heap bytes, not RSS or
Python allocations. Full pipeline benchmarks provide the separate RSS evidence.
"""

import argparse
import hashlib
import json
import os
import platform
import sys
from datetime import UTC, datetime
from pathlib import Path

from native_coverage import embedded_python_environment, run

ROOT = Path(__file__).resolve().parents[1]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--shapes", type=Path, required=True)
    parser.add_argument("--inference", choices=("rdfs", "owlrl", "both"), default="owlrl")
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    environment = embedded_python_environment(sys.executable, dict(os.environ))
    environment.update(
        PYO3_PYTHON=sys.executable,
        MEMORY_DATA=str(args.data.resolve(strict=True)),
        MEMORY_SHAPES=str(args.shapes.resolve(strict=True)),
        MEMORY_INFERENCE=args.inference,
    )
    log = run(
        [
            "cargo",
            "test",
            "--manifest-path",
            ROOT / "Cargo.toml",
            "--locked",
            "--release",
            "--features",
            "allocation-profiling",
            "allocation_tests::profile_pipeline_allocations",
            "--",
            "--ignored",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ],
        environment,
        capture=True,
    )
    phases = [
        json.loads(line.split("MEMORY_PHASE ", 1)[1])
        for line in log.splitlines()
        if "MEMORY_PHASE " in line
    ]
    if [row["phase"] for row in phases] != [
        "setup",
        "parse",
        "inference",
        "compile_shapes",
        "validation_native_report",
        "released",
    ]:
        raise RuntimeError(f"Incomplete allocation profile: {log}")
    report = {
        "schema": 1,
        "date": datetime.now(UTC).isoformat(),
        "kind": "requested-native-Rust-heap-bytes",
        "source_revision": run(["git", "-C", ROOT, "rev-parse", "HEAD"], capture=True).strip(),
        "source_tree_dirty": bool(
            run(
                ["git", "-C", ROOT, "status", "--porcelain", "--untracked-files=no"], capture=True
            ).strip()
        ),
        "platform": platform.platform(),
        "python": sys.version,
        "inference": args.inference,
        "fixtures": {
            name: {
                "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "bytes": path.stat().st_size,
            }
            for name, path in (("data", args.data), ("shapes", args.shapes))
        },
        "phases": phases,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    args.output.with_suffix(".log").write_text(log)
    print(json.dumps(phases, indent=2))


if __name__ == "__main__":
    main()
