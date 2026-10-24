"""The opt-in comparison must preserve evidence without turning noise into a gate."""

import importlib.util
import sys
import tempfile
import unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location(
    "comparison", ROOT / "scripts/inference_comparison.py"
)
comparison = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(comparison)
identity = __import__("comparison_identity")


class ComparisonTests(unittest.TestCase):
    def test_existing_action_dev_only_flags_do_not_change_release_profile(self):
        comparison.validate_flags({"CARGO_PROFILE_DEV_DEBUG": "0"})
        comparison.validate_flags({"CARGO_PROFILE_DEV_DEBUG": "0", "RUSTFLAGS": "-D warnings"})
        for value in [
            {"CARGO_PROFILE_RELEASE_DEBUG": "0"},
            {"CARGO_PROFILE_DEV_DEBUG": "1"},
            {"RUSTFLAGS": "-C opt-level=0"},
        ]:
            with self.assertRaises(ValueError):
                comparison.validate_flags(value)

    def test_preflight_failure_still_retains_exact_requested_inputs(self):
        with tempfile.TemporaryDirectory() as directory:
            output = Path(directory) / "output"
            with patch.object(comparison, "preflight", side_effect=ValueError("preflight failed")):
                with self.assertRaisesRegex(ValueError, "preflight failed"):
                    comparison.run_comparison(
                        SimpleNamespace(base="a" * 40, head="b" * 40, output=output)
                    )
            self.assertEqual(
                __import__("json").loads((output / "request.json").read_text())["base"], "a" * 40
            )

    def test_path_engine_source_has_a_clear_unsupported_error(self):
        metadata = {
            "resolve": {"nodes": []},
            "packages": [
                {
                    "name": "pyshacl-turbo-engine",
                    "manifest_path": "/unused/Cargo.toml",
                    "source": None,
                }
            ],
        }
        with self.assertRaisesRegex(ValueError, "immutable Git"):
            identity.derive(Path("/unused"), metadata)

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

    def test_mismatched_pair_fails_before_any_native_build(self):
        with tempfile.TemporaryDirectory() as directory:

            def checkout(repository, revision, destination, log):
                destination.mkdir()
                for name in comparison.PAIRED_INPUTS:
                    target = destination / name
                    target.parent.mkdir(parents=True, exist_ok=True)
                    target.write_text(revision if name == "Cargo.lock" else "identical")

            with (
                patch.object(comparison, "preflight"),
                patch.object(comparison, "owl_input", return_value=Path(directory) / "owl.whl"),
                patch.object(comparison, "checkout", side_effect=checkout),
                patch.object(comparison, "build") as build,
            ):
                with self.assertRaisesRegex(ValueError, "Cargo.lock"):
                    comparison.run_comparison(
                        SimpleNamespace(
                            base="a" * 40, head="b" * 40, output=Path(directory) / "output"
                        )
                    )
                build.assert_not_called()

    def test_summary_retains_wall_and_memory_without_noisy_cpu_ratio(self):
        results = []
        for arm in ("base", "head"):
            for index, cpu in enumerate((1, 1, 1, 2)):
                results.append(
                    {
                        "case": "ring",
                        "arm": arm,
                        "guard": None,
                        "records": [
                            {
                                "stage": "inferred",
                                "inference_cpu_ns": cpu * 10**9,
                                "inference_wall_ns": (index + 1) * 10**9,
                                "peak_before_export_bytes": 1024 * (index + 1),
                            }
                        ],
                    }
                )
        summary = comparison.summarize_results(results, ["ring"])["ring"]
        self.assertEqual(summary["status"], "inconclusive")
        self.assertNotIn("head_over_base_cpu_median", summary)
        self.assertEqual(summary["arms"]["base"]["wall_seconds"]["median"], 2.5)
        self.assertEqual(summary["arms"]["head"]["peak_before_export_bytes"]["median"], 2560)


if __name__ == "__main__":
    unittest.main()
