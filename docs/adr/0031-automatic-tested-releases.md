# ADR 0031: Automatic tested prereleases

Status: Accepted

Main CI is the sole publisher. Manual dispatch and the daily recovery schedule
resume that same pipeline; neither opts into an alternative publisher. PRs run
the complete validation graph without release writes.

Version selection and immutable tagging use the shared actions pinned at
`4225546e92bf7ca66814a00070db2193051473eb`. The planner considers scoped
Conventional Commits since the latest reachable release and rejects invalid
unreleased headers. Numeric alpha, beta and rc candidates advance within their
intended version for releasable changes; stable promotion is an explicit manifest
change. Historical PEP 440 tags remain unchanged (`v0.2.0a1` corresponds to
Cargo `0.2.0-alpha.1`). No parallel local version planner or tagger remains.

The repository-scoped release-writer App records GitHub-verified version commits
with an expected-head guard. `cargo version-info` invokes the declared companion
hook to synchronize `pyproject.toml` and `uv.lock`. Every downstream checkout
uses the returned revision, including the step exposing the Python version.
The shared tagger verifies that revision's Cargo manifest. Cargo is not a
separately published crate.

All 25 CPython/platform wheels (ADR 0020), source-archive tests, native coverage, exact-term
storage contracts and lint gates remain mandatory. Publication consumes the
original tested artifacts and retains the immutable archive bundle and hashes
in a draft GitHub release. Recovery finishes an incomplete tag before advancing
to another version and restores the original bundle rather than replacing it
with newly built bytes. Only missing PyPI files are uploaded; unexpected files,
hash differences and service/authentication failures are hard errors. Only a
404 can mean an absent GitHub release. Finalize the draft after registry hash
verification succeeds.

Shared engine source fingerprints remain the compatibility contract between
companion wheels. Automatic publication makes updated rdflib-turbo engines
available after successful main CI; it does not imply that independently
released companion versions share a numeric version or that every latest
registry combination is compatible.
