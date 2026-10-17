# Native coverage and live badges

Main-branch CI and its daily schedule generate Python line/branch coverage and
owned Rust line/region/function coverage. Successful checks publish reports and
five measured badges to [GitHub Pages](https://legra-ai.github.io/rdflib-turbo/).
PRs retain artifacts without publishing; failures leave the last successful
snapshot. Public reports record their revision/date, and badge values are never
hard-coded into the README.

Install Rust 1.99.1's llvm-tools-preview component and cargo-llvm-cov 0.9.1, then
run `make test-rust-coverage` with the sibling owlrl-turbo checkout present.
The standalone command is `python scripts/native_coverage.py --owlrl-source ../owlrl-turbo`.
It uses pinned coverage dependencies and a fresh venv, builds and verifies an
instrumented wheel, runs Rust/native Python/OWL-RL Python tests and merges actual
counters. Normal development extensions are not replaced.

The initial baseline had zero standalone Rust test cases. Standalone tests now
exercise graph helpers, inference boundaries, error propagation and lock poisoning.
The native and OWL-RL Python suites exercise different parts of this shared implementation. Their percentages must not be summed.
Reports retain all three stages and the combined result. CI pins the companion
source revision explicitly; OWL-RL's own coverage job tests its current source.

`native-coverage/` contains JSON/Markdown summaries, per-file counts, detailed
LLVM exports, merged profiles and HTML. The [first baseline](native-coverage-baseline.json)
records source hashes and tool versions. Every owned src/*.rs file must appear.
Each report's denominator is checked against LLVM's complete empty-profile
mapping of the same binaries. Loaded wheel bytes and actual Python-to-Rust
execution are verified. Missing measurements fail rather than reuse old numbers.

Stable instrumentation measures lines, regions and functions. Rust branches,
dependency/vendored-patch coverage and conformance are not measured by these
badges. External PyO3 bootstrap mapping diagnostics remain visible in CI logs;
the owned-source denominator must still match the complete mapping. Python
coverage retains its 100% line/branch requirement. No numerical Rust floor is
invented for this first baseline. Badge values round down to a tenth.


Standalone native tests now cover internal graph helpers, malformed declarations,
inference atomicity and boundary failures alongside public Python integration.
Remaining unexecuted regions stay in the denominator. Examples include errors
from serializing valid RDF into an in-memory byte vector, parsing built-in query
constants and impossible result variants after query-kind validation. These are
not assigned synthetic hits. The HTML report is the per-revision gap inventory;
a high aggregate percentage does not establish exhaustive behavioral coverage.

The Rust test executables embed the selected Python distribution. Their runtime
uses that interpreter's absolute base prefix, separately from the wheel-test
venv, so relocatable Python installations load the matching standard library.
The shared datatype parser correction and required upstream tests are recorded
in the datatype-parser ADR and vendored patch note.
