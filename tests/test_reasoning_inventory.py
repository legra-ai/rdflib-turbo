"""The reviewed standard-rule inventory must not silently omit table rows."""

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class RuleInventoryTests(unittest.TestCase):
    def test_every_standard_rule_has_one_explicit_ownership_row(self):
        expected = set((ROOT / "tests/fixtures/owl2-rl-rule-ids.txt").read_text().splitlines())
        document = (ROOT / "docs/reasoning-conformance.md").read_text()
        rows = re.findall(
            r"^\| `((?:eq|prp|cls|cax|dt|scm)-[a-z0-9-]+)` \|", document, re.MULTILINE
        )
        self.assertEqual(len(expected), 78)
        self.assertEqual(len(rows), len(set(rows)))
        self.assertEqual(set(rows), expected)
