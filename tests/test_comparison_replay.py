"""Replay only authenticated original public inputs with compatible hosts."""

import importlib
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from zipfile import ZipFile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))


class ReplayTests(unittest.TestCase):
    def module(self):
        return importlib.import_module("comparison_replay_inputs")

    def test_origin_requires_successful_public_dispatch_and_exact_build_harness(self):
        module = self.module()
        valid = {
            "event": "workflow_dispatch",
            "conclusion": "success",
            "head_sha": module.BUILD_HARNESS,
            "path": ".github/workflows/ci.yml",
            "head_repository": {"full_name": "legra-ai/rdflib-turbo"},
        }
        module.validate_origin(valid)
        for key, value in [
            ("event", "pull_request"),
            ("conclusion", "failure"),
            ("head_sha", "a" * 40),
            ("path", "other.yml"),
            ("head_repository", {"full_name": "other/repo"}),
        ]:
            with self.assertRaises(ValueError):
                module.validate_origin({**valid, key: value})

    def test_unsafe_zip_path_never_extracts(self):
        module = self.module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / "input.zip"
            with ZipFile(archive, "w") as bundle:
                bundle.writestr("../escape", "unsafe")
            with self.assertRaises(ValueError):
                module.extract_safe(archive, root / "extracted")
            self.assertFalse((root / "escape").exists())

    def test_host_isa_must_cover_every_original_target_feature(self):
        module = self.module()
        module.require_features(["fxsr", "sse", "sse2"], {"fxsr", "sse", "sse2", "avx2"})
        with self.assertRaises(ValueError):
            module.require_features(["sse2"], {"sse"})
        with self.assertRaises(ValueError):
            module.require_features(["unknown-feature"], {"unknown-feature"})

    def test_replay_classifies_all_eight_samples_without_dropping_values(self):
        module = importlib.import_module("comparison_replay")
        stable = [1.0] * 8
        self.assertEqual(module.measurement(stable)["status"], "complete")
        noisy = [1.0] * 7 + [2.0]
        actual = module.measurement(noisy)
        self.assertEqual(actual["status"], "inconclusive")
        self.assertEqual(actual["values"], noisy)
        self.assertEqual(module.measurement(stable[:7])["status"], "inconclusive")

    def test_wrong_source_request_stops_before_replay_or_checkout(self):
        module = self.module()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "request.json").write_text(
                json.dumps({"base": "wrong", "head": "b" * 40, "python": "3.13.16"})
            )
            with patch.object(module, "checkout") as checkout:
                with self.assertRaises(ValueError):
                    module.verify(root, root, "a" * 40, "b" * 40, root)
                checkout.assert_not_called()

    def test_zip_digest_failure_stops_before_extraction(self):
        module = self.module()
        origin = {
            "event": "workflow_dispatch",
            "conclusion": "success",
            "head_sha": module.BUILD_HARNESS,
            "path": ".github/workflows/ci.yml",
            "head_repository": {"full_name": "legra-ai/rdflib-turbo"},
        }
        artifacts = {
            "artifacts": [
                {
                    "name": "performance-comparison-inputs",
                    "id": 1,
                    "expired": False,
                    "digest": "sha256:" + "0" * 64,
                }
            ]
        }
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch.object(
                    module.subprocess,
                    "check_output",
                    side_effect=[json.dumps(origin).encode(), json.dumps(artifacts).encode()],
                ),
                patch.object(module.subprocess, "run"),
                patch.object(module, "extract_safe") as extract,
            ):
                with self.assertRaises(ValueError):
                    module.download(1, Path(directory))
                extract.assert_not_called()

    def test_replay_uses_existing_opt_in_job_and_separate_artifact_condition(self):
        workflow = (Path(__file__).resolve().parents[1] / ".github/workflows/ci.yml").read_text()
        comparison = workflow.rsplit("\n  inference-comparison:\n", 1)[1]
        original = comparison.split("      - name: Retain original public inputs", 1)[1].split(
            "      - name:", 1
        )[0]
        self.assertIn("if: always() && inputs['comparison-replay-run'] == ''", original)
        self.assertIn("if: inputs['comparison-replay-run'] != ''", comparison)
        self.assertIn("name: performance-comparison-replay", comparison)
        self.assertIn("actions: read", comparison)
        self.assertNotIn("actions: write", comparison)
        self.assertNotIn("release-wheel", comparison)


if __name__ == "__main__":
    unittest.main()
