"""Preserve upstream matcher suites and the opt-in Python VM's actual regressions."""

import subprocess
from pathlib import Path

UNICODE_GUARD = "vm::tests::casei_unicode_fold_equality"
PYTHON_GUARDS = (
    "vm::python_re_tests::variable_width_lookbehind_uses_python_compile_error",
    "vm::python_re_tests::unicode_backreferences_advance_by_characters_and_compare_lowercase",
    "vm::python_re_tests::ascii_backreferences_preserve_non_ascii_and_reject_invalid_boundaries",
)
# bounded: the fixed default/Python VM regression inventory.
ALL_GUARDS = (UNICODE_GUARD, *PYTHON_GUARDS)


def run(root):
    manifest = str(root / "vendor/fancy-regex-0.20.0/Cargo.toml")
    profiles = (
        ([], (UNICODE_GUARD,)),
        (["--lib", "--no-default-features", "--features", "std,perf,unicode"], (UNICODE_GUARD,)),
        (
            [
                "--lib",
                "--no-default-features",
                "--features",
                "std,perf,unicode,python-re",
            ],
            PYTHON_GUARDS,
        ),
    )
    for flags, guards in profiles:
        command = [
            "cargo",
            "--config",
            'build.warnings="deny"',
            "test",
            "--manifest-path",
            manifest,
            "--locked",
            *flags,
        ]
        listed = subprocess.run(
            [*command, "--", "--list"], cwd=root, check=True, stdout=subprocess.PIPE, text=True
        ).stdout
        available = {
            line.removesuffix(": test") for line in listed.splitlines() if line.endswith(": test")
        }
        missing = set(guards) - available
        if missing:
            raise RuntimeError(f"Missing matcher regressions: {sorted(missing)}")
        subprocess.run(command, cwd=root, check=True)
    formatter = (root / "rustfmt-toolchain").read_text().strip()
    if formatter != "nightly-2026-10-16":
        raise ValueError("Matcher formatting requires the existing pinned nightly")
    subprocess.run(
        ["cargo", "+" + formatter, "fmt", "--manifest-path", manifest, "--", "--check"],
        cwd=root,
        check=True,
    )


if __name__ == "__main__":
    run(Path(__file__).resolve().parents[1])
