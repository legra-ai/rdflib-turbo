"""Select the exact wheel layout owned by the prepared, possibly historical source."""

import json
import sys
import tomllib
from pathlib import Path

VERSIONS = "3.11 3.12 3.13 3.14 3.15"
PLATFORMS = (
    ("ubuntu-24.04", "x86_64-unknown-linux-gnu", "quay.io/pypa/manylinux_2_28_x86_64:latest"),
    ("ubuntu-24.04-arm", "aarch64-unknown-linux-gnu", "quay.io/pypa/manylinux_2_28_aarch64:latest"),
    ("macos-15-intel", "x86_64-apple-darwin", None),
    ("macos-15", "aarch64-apple-darwin", None),
    ("windows-latest", "x86_64-pc-windows-msvc", None),
)


def select(project):
    features = project["tool"]["maturin"]["features"]
    original = {"pyo3/extension-module", "persistence"}
    if set(features) == original | {"pyo3/abi3-py311"}:
        stable = True
        required = ">=3.11,<3.16"
    elif set(features) == original:
        # Concrete historical releases retain their original tested inventory.
        stable = False
        required = ">=3.11"
    else:
        raise ValueError("Unknown prepared-source native wheel feature policy")
    if project["project"]["requires-python"].replace(" ", "") != required:
        raise ValueError("Prepared Python range differs from the audited wheel policy")
    rows = []  # bounded: five fixed platforms and at most two historical cohorts.
    for os_name, target, container in PLATFORMS:
        if stable and target == "x86_64-apple-darwin":
            continue
        cohorts = [VERSIONS]
        if not stable and os_name in {"macos-15-intel", "windows-latest"}:
            cohorts = ["3.11 3.12 3.13", "3.14 3.15"]
        for versions in cohorts:
            artifact = "release-wheel-" + target
            if len(cohorts) == 2:
                artifact += "-cp311-cp313" if versions.startswith("3.11") else "-cp314-cp315"
            row = {
                "os": os_name,
                "target": target,
                "python-versions": versions,
                "artifact": artifact,
            }
            if container:
                row["container"] = container
            rows.append(row)
    return {"stable-abi": stable, "matrix": {"include": rows}}


def main():
    if len(sys.argv) != 2:
        raise ValueError("Expected the GitHub output file")
    selected = select(tomllib.loads(Path("pyproject.toml").read_text()))
    with Path(sys.argv[1]).open("a") as output:
        output.write("stable-abi=" + str(selected["stable-abi"]).lower() + "\n")
        output.write("wheel-matrix=" + json.dumps(selected["matrix"], separators=(",", ":")) + "\n")


if __name__ == "__main__":
    main()
