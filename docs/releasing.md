# Releases

`ci.yml` is the only PyPI publisher. Successful releasable main changes publish
automatically; manual workflow dispatch and the daily schedule resume the same
path. PRs validate without publishing. The public companion repositories are
[pyshacl-turbo](https://github.com/legra-ai/pyshacl-turbo),
[rdflib-turbo](https://github.com/legra-ai/rdflib-turbo) and
[owlrl-turbo](https://github.com/legra-ai/owlrl-turbo).

The shared release planner and tested tagger are pinned to
`4225546e92bf7ca66814a00070db2193051473eb`. On main the repository-scoped
release-writer App uses an expected-head guard to record a verified version
commit. `cargo version-info` runs the declared `scripts/sync_version.py` hook,
updating the Python version and `uv.lock` with Cargo. Every test, build and
publication checks out the exact revision returned by preparation.

For prereleases, releasable changes advance the candidate number: Cargo
`0.2.0-alpha.1` becomes `0.2.0-alpha.2`, Python `0.2.0a2`, tag `v0.2.0a2`.
The intended version/channel and stable promotion are explicit manifest choices.
Stable releases follow the shared major/minor/patch Conventional Commit policy.
Documentation, style and CI-only changes do not independently publish. Every
unreleased commit header must satisfy the scoped Conventional Commit policy;
historical release tags are not rewritten.

The pipeline builds source distributions and 25 CPython/platform wheels with `uv build`,
tests installed artifacts, checks metadata and native behavior, and requires
native coverage and exact-term storage contracts before publishing. The native
Cargo package remains unpublished. Original code uses MIT OR Apache-2.0.

The publishing job retains the exact tested archives in `release-packages.zip`
on a draft GitHub release. A retry resumes the original tagged source before
preparing another version, restores those original bytes and uploads only missing
PyPI files. Existing files must match their recorded SHA-256 identities. Service,
authentication, source or hash conflicts fail the pipeline; the GitHub release
remains draft until the required registry files are verified. The publisher never
builds replacement packages or overwrites the archive bundle.

PyPI trusted publication uses owner `legra-ai`, repository `rdflib-turbo`,
workflow `ci.yml` and environment `pypi`, which permits main only. `uv publish`
uses that publisher with PEP 740 attestations for the original tested archives.
No local upload bypass is used. Dependabot checks Cargo, uv and Actions daily.

Shared engine fingerprints must match across companion wheels; package version
numbers need not match. Automatic releases keep tested engine updates from being
stranded behind a manual publishing gate. They do not make mismatched native
engines interchangeable.

Release wheels target CPython 3.11, 3.12, 3.13, 3.14 and 3.15 separately on each
of the five existing platforms. Each platform job unpacks the tested source
archive once and reuses its native build cache across all interpreters. Every
wheel is installed and tested under its matching interpreter, including checks
of the loaded package/native bytes and 100% Python line and branch coverage.
The final inventory requires exactly 25 interpreter/platform wheels and one
source archive. ABI3, mismatched, missing and additional archives fail the gate.
A previously published version's original archive bundle remains immutable;
a changed inventory belongs to a new release version.

See [ADR 0031](adr/0031-automatic-tested-releases.md) and
[ADR 0020](adr/0020-build-cpython-specific-wheels.md). Normal development uses
`uv sync --locked --extra test` and `uv run --locked --extra test`.
