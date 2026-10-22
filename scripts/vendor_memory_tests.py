"""Run actual vendored storage unit tests with the project's dependency patches."""

import json
import subprocess
import tomllib
from pathlib import Path

# bounded: the fixed storage contracts must never silently select zero tests.
REQUIRED_TESTS = (
    "storage::memory::missing_pattern_tests::missing_pattern_stops_probing_indexes_after_first_absent_component",
    "storage::memory::missing_pattern_tests::missing_pattern_reads_follow_commits_rollback_and_retained_snapshots",
    "storage::memory::cardinality_tests::cardinality_snapshot_len_does_not_visit_physical_history",
    "storage::memory::cardinality_tests::cardinality_snapshot_pair_matches_its_visible_rows_during_publication",
    "storage::lexical_hash_tests::lexical_hash_vectors_are_stable",
    "storage::rocksdb::key_hash_tests::fresh_store_declares_its_lexical_hash_scheme",
    "storage::rocksdb::key_hash_tests::mismatched_hash_scheme_fails_for_writers_and_readers",
    "storage::rocksdb::key_hash_tests::missing_hash_scheme_is_not_inferred_from_existing_data",
    "storage::rocksdb::key_hash_tests::hash_scheme_without_storage_version_is_rejected_without_repair",
    "storage::rocksdb::key_hash_tests::undeclared_orphan_data_is_rejected_in_every_column_family",
)


def patch_arguments(root):
    configuration = tomllib.loads((root / "Cargo.toml").read_text())
    # Match CI's warning policy for standalone vendor test crates as well.
    arguments = ["--config", 'build.warnings="deny"']
    for package, patch in configuration["patch"]["crates-io"].items():
        for key, value in patch.items():
            if key == "path":
                value = str((root / value).resolve())
            arguments.extend(["--config", f"patch.crates-io.{package}.{key}={json.dumps(value)}"])
    return arguments


def run_configuration(root, features, required):
    arguments = [
        "cargo",
        *patch_arguments(root),
        "test",
        "--locked",
        "--manifest-path",
        str(root / "vendor/oxigraph-0.5.11/Cargo.toml"),
        "--no-default-features",
    ]
    if features is not None:
        arguments.extend(["--features", features])
    arguments.extend(["--lib", "storage::"])
    listed = subprocess.run(
        [*arguments, "--", "--list"], cwd=root, check=True, stdout=subprocess.PIPE, text=True
    ).stdout
    available = {
        line.removesuffix(": test") for line in listed.splitlines() if line.endswith(": test")
    }
    missing = set(required) - available
    if missing:
        raise RuntimeError(f"Missing native storage regressions: {sorted(missing)}")
    subprocess.run(arguments, cwd=root, check=True)


def run(root):
    run_configuration(root, "exact-term-storage,rocksdb", REQUIRED_TESTS)
    run_configuration(
        root,
        "rocksdb",
        (
            "storage::numeric_encoder::tests::str_hash_stability",
            "storage::rocksdb::upstream_hash_tests::exact_hash_declarations_are_rejected_before_upstream_migration",
        ),
    )


if __name__ == "__main__":
    run(Path(__file__).resolve().parents[1])
