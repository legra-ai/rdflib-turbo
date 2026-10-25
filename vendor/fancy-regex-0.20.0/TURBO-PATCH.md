# Local Python regular-expression semantics

Published fancy-regex 0.20.0, with original revision and archive checksum in
`../rust-sources.json`. Its MIT license is retained. No upstream PR is submitted.

The opt-in `python-re` feature requires fixed-width lookbehind before any
upstream alternation rewriting and adds Python-compatible case-insensitive
backreferences and repeatable lookarounds. Backreferences compare simple
lowercase codepoints and advance by characters, so `i` can match `İ` despite
different UTF-8 byte lengths. The scoped `a` flag limits backreference case
matching to ASCII. The parser carries this mode through the AST to the VM.
With this feature disabled, existing matching behavior is retained; the added
AST field defaults to false in the retained upstream tests.

Python-mode callers may supply an owned `Arc<dyn PythonCaseMapping>` through
the regex builder. The VM program retains this immutable native scalar policy
for Unicode case-insensitive backreferences, including across regex clones.
Scoped ASCII backreferences continue to use ASCII comparison. This policy is
not the larger case closure used for translated Python literals and sets.
No interpreter lookup or Python callback runs while matching. Omitting this
option retains the existing behavior for independent callers; the host SHACL
integration must explicitly provide its interpreter's policy.

The SHACL engine separately translates Python character classes, flags and
anchors before compilation. This is not a general Python mode for unmodified
patterns passed directly to fancy-regex. Its seek optimization remains disabled:
the upstream case-folding approximation is not valid for Python backreferences.

Regressions are in `../../tests/test_patterns.py` and the private VM unit tests
in `src/vm.rs`. The full dependency test suite runs with default features; the
full unit suite runs with `--no-default-features --features std,perf,unicode,python-re`.

The root workspace explicitly excludes this crate so its tests can run directly
with an independent feature selection. Its retained `Cargo.lock` pins the test
dependencies. From the repository root, `make test-regex` runs the full upstream
suite, the unit suite with Turbo's base features and Python mode disabled, and
the Python-mode VM regressions. The Regex workflow runs these checks on every PR,
main push, and daily schedule.
