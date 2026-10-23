"""The opt-in comparison must preserve evidence without turning noise into a gate."""

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "comparison", ROOT / "scripts/inference_comparison.py"
)
comparison = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(comparison)


class ComparisonTests(unittest.TestCase):
    def test_requires_distinct_full_public_commits(self):
        comparison.validate_refs("a" * 40, "b" * 40)
        for base, head in [("main", "b" * 40), ("a" * 40, "a" * 40), ("A" * 40, "b" * 40)]:
            with self.assertRaises(ValueError):
                comparison.validate_refs(base, head)

    def test_cold_order_is_balanced_and_predeclared(self):
        self.assertEqual(
            comparison.ARMS, ("base", "head", "head", "base", "head", "base", "base", "head")
        )

    def test_incomplete_or_noisy_timing_is_inconclusive(self):
        for values in [(1, 1, 1), (1, 1, 1, 2)]:
            self.assertEqual(comparison.timing_summary(list(values))["status"], "inconclusive")
        self.assertEqual(comparison.timing_summary([1, 1, 1, 1])["status"], "complete")

    def test_witness_mismatch_is_hard_failure(self):
        with self.assertRaises(ValueError):
            comparison.require_equal_witnesses([{"count": 1}, {"count": 2}])
        comparison.require_equal_witnesses([{"count": 1}, {"count": 1}])

    def test_archive_audit_rejects_missing_or_changed_runtime(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ["source", "archive"]:
                (root / name / "src").mkdir(parents=True)
                (root / name / "src/lib.rs").write_text("original")
                (root / name / "Cargo.toml").write_text('[package]\nname="example"\n')
            comparison.audit_tree(root / "source", root / "archive", ["src/lib.rs", "Cargo.toml"])
            (root / "archive/src/lib.rs").write_text("changed")
            with self.assertRaises(ValueError):
                comparison.audit_tree(
                    root / "source", root / "archive", ["src/lib.rs", "Cargo.toml"]
                )
            (root / "archive/src/lib.rs").unlink()
            with self.assertRaises(ValueError):
                comparison.audit_tree(
                    root / "source", root / "archive", ["src/lib.rs", "Cargo.toml"]
                )

    def test_only_inferred_readme_manifest_normalization_is_allowed(self):
        original = {"package": {"name": "example"}}
        comparison.normalized_manifest(
            original, {"package": {"name": "example", "readme": "README.md"}}
        )
        with self.assertRaises(ValueError):
            comparison.normalized_manifest(original, {"package": {"name": "other"}})

    def test_workflow_is_opt_in_and_separate_from_publisher(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        self.assertIn("comparison-base:", workflow)
        self.assertIn("comparison-head:", workflow)
        job = workflow.rsplit("\n  inference-comparison:", 1)[1]
        self.assertIn("github.event_name == 'workflow_dispatch'", job)
        self.assertIn("performance-comparison-inputs", job)
        self.assertNotIn("release-sdist", job)
        self.assertNotIn("release-wheel", job)
        self.assertNotIn("id-token:", job)
        self.assertIn("needs: [prepare, verify, sdist, wheels, native-coverage]", workflow)

    def test_only_explicit_comparison_dispatch_skips_default_graph(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        condition = "github.event_name != 'workflow_dispatch' || !inputs['inference-comparison']"
        self.assertIn("  prepare:\n    if: " + condition, workflow)
        self.assertIn("if: always() && (" + condition + ")", workflow)


if __name__ == "__main__":
    unittest.main()
