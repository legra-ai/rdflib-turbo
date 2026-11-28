"""Cargo.lock holds one release of each shared model crate (ADR 0079)."""

import tomllib
import unittest
from collections import defaultdict
from pathlib import Path

MODEL_CRATES = frozenset(
    {
        "datalog-structural-model",
        "inference-engine",
        "iri-rfc3987",
        "owl-structural-model",
        "rdf-reasoning",
        "rdf-structural-model",
        "shacl-structural-model",
        "sparql-results",
    }
)


class ModelCrateVersionTests(unittest.TestCase):
    def test_one_release_per_model_crate(self):
        root = Path(__file__).resolve().parents[1]
        lock = tomllib.loads((root / "Cargo.lock").read_text())
        versions = defaultdict(set)
        for package in lock["package"]:
            if package["name"] in MODEL_CRATES:
                versions[package["name"]].add(package["version"])
        duplicated = {name: sorted(found) for name, found in versions.items() if len(found) > 1}
        self.assertEqual(duplicated, {}, "Align the model crates on one release each")


if __name__ == "__main__":
    unittest.main()
