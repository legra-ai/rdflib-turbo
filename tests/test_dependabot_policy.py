"""Keep immutable shared policy revisions coherent without filtering SHA updates."""

import unittest
from pathlib import Path

import yaml

ROOT = Path(__file__).resolve().parents[1]


class DependabotPolicyTests(unittest.TestCase):
    def test_shared_policy_sha_updates_form_one_version_group(self):
        updates = yaml.safe_load((ROOT / ".github/dependabot.yml").read_text())["updates"]
        actions = next(x for x in updates if x["package-ecosystem"] == "github-actions")
        groups = actions["groups"]
        shared = groups["shared-policy"]
        self.assertEqual(shared["applies-to"], "version-updates")
        self.assertEqual(shared["patterns"], ["legra-ai/github-actions*"])
        self.assertNotIn("update-types", shared)
        self.assertIn("legra-ai/github-actions*", groups["compatible"]["exclude-patterns"])

    def test_daily_security_and_commit_contracts_remain_covered(self):
        updates = yaml.safe_load((ROOT / ".github/dependabot.yml").read_text())["updates"]
        self.assertEqual(
            {x["package-ecosystem"] for x in updates}, {"cargo", "uv", "github-actions"}
        )
        for update in updates:
            self.assertEqual(update["schedule"]["interval"], "daily")
            self.assertEqual(update["groups"]["security"]["applies-to"], "security-updates")
            self.assertEqual(update["groups"]["security"]["patterns"], ["*"])
            self.assertEqual(update["commit-message"]["include"], "scope")
            self.assertEqual(
                update["commit-message"]["prefix"],
                "ci" if update["package-ecosystem"] == "github-actions" else "build",
            )
        cargo = next(x for x in updates if x["package-ecosystem"] == "cargo")
        self.assertIn({"dependency-type": "all"}, cargo["allow"])


if __name__ == "__main__":
    unittest.main()
