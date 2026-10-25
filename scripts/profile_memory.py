"""Measure native Rust allocation peaks by phase in an isolated release test binary.

Run through uv with the selected Python interpreter. No profiling code is enabled
in ordinary tests or wheels. Counts are requested Rust heap bytes, not RSS or
Python allocations. Full pipeline benchmarks provide the separate RSS evidence.
"""

import argparse
import hashlib
import json
import platform
import sys
import tomllib
import uuid
from datetime import UTC, datetime
from pathlib import Path

from native_coverage import locked_python_environment, run

ROOT = Path(__file__).resolve().parents[1]


def profile_case(owner):
    return {
        "canonical-store": "allocation_tests::canonical::profile_canonical_store_pipeline_allocations",
        "generalized-dataset": "allocation_tests::profile_pipeline_allocations",
    }[owner]


def source_inputs(destination):
    import ci_facade

    builder = ci_facade.load("build_facade_candidate")
    selected = ci_facade.policy()
    if "declaration" not in selected:
        raise ValueError("Profile export requires its independent declaration authority")
    provider_revision = ci_facade.run(
        ["git", "-C", ROOT, "rev-parse", "HEAD"], capture=True
    ).strip()
    provider_input = builder.freeze(ROOT, provider_revision, "rdflib-turbo", ci_facade.run)
    cohort = destination / "export"
    cohort.mkdir(parents=True)
    manifest = tomllib.loads((ROOT / "pyproject.toml").read_text())
    declaration = cohort / builder.peer_name(manifest, "pyshacl-turbo")
    ci_facade.checkout(selected["declaration"], declaration)
    declared = tomllib.loads((declaration / "pyproject.toml").read_text())
    provider = cohort / builder.peer_name(declared, "rdflib-turbo")
    ci_facade.checkout({"repository": str(ROOT), "revision": provider_revision}, provider)
    if builder.freeze(provider, provider_revision, "rdflib-turbo", ci_facade.run) != provider_input:
        raise RuntimeError("Export provider differs from the compiled source owner")
    sources = {"rdflib-turbo": provider, "declaration": declaration}
    for role, name in (("facade", "pyshacl-turbo"), ("owlrl", "owlrl-turbo")):
        source = destination / role
        source.parent.mkdir(parents=True, exist_ok=True)
        ci_facade.checkout(selected[role], source)
        sources[name] = source
    records = {}
    for name, source in sources.items():
        revision = ci_facade.run(["git", "-C", source, "rev-parse", "HEAD"], capture=True).strip()
        records[name] = {
            "source": str(source),
            "project": "pyshacl-turbo" if name == "declaration" else name,
            **builder.freeze(
                source, revision, "pyshacl-turbo" if name == "declaration" else name, ci_facade.run
            ),
        }
    records["rdflib-turbo"]["compiled_source"] = str(ROOT)
    return records


def verify_source_inputs(records):
    import ci_facade

    builder = ci_facade.load("build_facade_candidate")
    for name, record in records.items():
        actual = builder.freeze(
            Path(record["source"]), record["revision"], record["project"], ci_facade.run
        )
        if actual != {key: record[key] for key in ("revision", "files")}:
            raise RuntimeError("Profile source inputs changed: " + name)
        if (
            name == "rdflib-turbo"
            and builder.freeze(ROOT, record["revision"], name, ci_facade.run) != actual
        ):
            raise RuntimeError("Compiled source owner changed during profile")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--owner", choices=("canonical-store", "generalized-dataset"), required=True
    )
    parser.add_argument("--data", type=Path, required=True)
    parser.add_argument("--shapes", type=Path, required=True)
    parser.add_argument("--inference", choices=("rdfs", "owlrl", "both"), default="owlrl")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--yaml-authority", type=Path, required=True)
    args = parser.parse_args()
    runtime_output = args.output.parent / (args.output.stem + "-runtime-" + uuid.uuid4().hex)
    sources = (
        source_inputs(runtime_output.with_name(runtime_output.name + "-sources"))
        if args.owner == "canonical-store"
        else None
    )
    dependency_root = Path(sources["rdflib-turbo"]["source"]) if sources is not None else ROOT
    with locked_python_environment(
        dependency_root, runtime_output, sys.executable, args.yaml_authority
    ) as environment:
        environment.update(
            MEMORY_DATA=str(args.data.resolve(strict=True)),
            MEMORY_SHAPES=str(args.shapes.resolve(strict=True)),
            MEMORY_INFERENCE=args.inference,
        )
        if sources is not None:
            authority = runtime_output / "source-inputs.json"
            authority.write_text(json.dumps(sources, indent=2) + "\n")
            environment.update(MEMORY_SOURCE_INPUTS=str(authority), PYTHONDONTWRITEBYTECODE="1")
            verify_source_inputs(sources)
        log = run(
            [
                "cargo",
                "test",
                "--package",
                "rdflib-turbo-native",
                "--lib",
                "--manifest-path",
                ROOT / "Cargo.toml",
                "--locked",
                "--release",
                "--features",
                "allocation-profiling",
                profile_case(args.owner),
                "--",
                "--ignored",
                "--exact",
                "--nocapture",
                "--test-threads=1",
            ],
            environment,
            capture=True,
        )
        if sources is not None:
            verify_source_inputs(sources)
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
        "validation_public_report"
        if args.owner == "canonical-store"
        else "validation_native_report",
        "released",
    ]:
        raise RuntimeError(f"Incomplete allocation profile: {log}")
    semantics = [
        json.loads(line.split("MEMORY_SEMANTICS ", 1)[1])
        for line in log.splitlines()
        if line.startswith("MEMORY_SEMANTICS ")
    ]
    if args.owner == "canonical-store" and (
        len(semantics) != 1 or semantics[0].get("native_origin") != "compiled-source-test-binary"
    ):
        raise RuntimeError("Canonical profile did not complete its source/semantic guards")
    report = {
        "schema": 1,
        "date": datetime.now(UTC).isoformat(),
        "kind": "requested-native-Rust-heap-bytes",
        "owner": args.owner,
        "source_inputs": sources,
        "semantic_observation": semantics[0] if semantics else None,
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
