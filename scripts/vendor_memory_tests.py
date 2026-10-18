"""Run actual vendored memory unit tests with the project's dependency patches."""

import json
import subprocess
import tomllib
from pathlib import Path

# bounded: the two deterministic cardinality regressions must never silently select zero tests.
REQUIRED_TESTS = (
    "storage::memory::cardinality_tests::cardinality_snapshot_len_does_not_visit_physical_history",
    "storage::memory::cardinality_tests::cardinality_snapshot_pair_matches_its_visible_rows_during_publication",
)


def patch_arguments(root):
    configuration = tomllib.loads((root / "Cargo.toml").read_text())
    arguments = []
    for package, patch in configuration["patch"]["crates-io"].items():
        for key, value in patch.items():
            if key == "path":
                value = str((root / value).resolve())
            arguments.extend(["--config", f"patch.crates-io.{package}.{key}={json.dumps(value)}"])
    return arguments


def run(root):
    arguments = [
        "cargo",
        *patch_arguments(root),
        "test",
        "--locked",
        "--manifest-path",
        str(root / "vendor/oxigraph-0.5.11/Cargo.toml"),
        "--no-default-features",
        "--features",
        "exact-term-storage",
        "--lib",
        "storage::memory",
    ]
    listed = subprocess.run(
        [*arguments, "--", "--list"], cwd=root, check=True, capture_output=True, text=True
    ).stdout
    available = {
        line.removesuffix(": test") for line in listed.splitlines() if line.endswith(": test")
    }
    missing = set(REQUIRED_TESTS) - available
    if missing:
        raise RuntimeError(f"Missing native memory regressions: {sorted(missing)}")
    subprocess.run(arguments, cwd=root, check=True)


if __name__ == "__main__":
    run(Path(__file__).resolve().parents[1])
