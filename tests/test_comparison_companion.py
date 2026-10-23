"""A public-source test companion must keep its own original artifact identity."""

import importlib
import sys
import tempfile
import unittest
from pathlib import Path
from zipfile import ZipFile

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))


class CompanionTests(unittest.TestCase):
    def test_wheel_payload_must_equal_exact_public_source_including_typed_marker(self):
        companion = importlib.import_module("comparison_companion")
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source"
            package = source / "src/owlrl_turbo"
            package.mkdir(parents=True)
            (package / "__init__.py").write_text("public-code")
            (package / "py.typed").write_text("")
            wheel = Path(directory) / "companion.whl"
            with ZipFile(wheel, "w") as archive:
                archive.writestr("owlrl_turbo/__init__.py", "public-code")
                archive.writestr("owlrl_turbo/py.typed", "")
            self.assertEqual(len(companion.verify_payload(source, wheel)), 2)
            with ZipFile(wheel, "w") as archive:
                archive.writestr("owlrl_turbo/__init__.py", "different-code")
            with self.assertRaisesRegex(ValueError, "public source"):
                companion.verify_payload(source, wheel)


if __name__ == "__main__":
    unittest.main()
