# Releases

The sibling repositories are /Users/jgeluk/Work/pyshacl-turbo, rdflib-turbo and
owlrl-turbo, with public repositories under legra-ai. Original code has the same
MIT OR Apache-2.0 license and owner review/signature conventions. Scoped
Conventional Commit headers are checked locally and on PRs. Dependabot checks
Cargo (where applicable), Python and Actions dependencies daily.

ci.yml is the only PyPI publisher. Initial functional alphas use a manually
selected PEP 440 manifest version, validated against the Rust alpha version
where applicable. Publication is an explicit workflow_dispatch with publish=true
on main after a full successful test/build cycle. This initial alpha workflow
does not use pyshacl-turbo's stable Cargo-only automatic version bump action,
which rejects alpha versions and is inapplicable to the Python-only owlrl facade.

The pipeline tests installed wheels, checks metadata and native behavior,
builds wheels from the source archive, retains the exact tested artifacts in
a draft GitHub prerelease, and verifies registry SHA-256 identities. Retries
restore the original archives and upload only missing files. Failures leave the
GitHub release in draft. No local PyPI upload bypass is used.

Configure the initial pending trusted publisher in PyPI with:

- Project: rdflib-turbo
- GitHub owner: legra-ai
- Repository: rdflib-turbo
- Workflow: ci.yml
- Environment: pypi

The pypi GitHub environment permits main only. The first successful upload
creates the PyPI project. A pending publisher alone does not reserve its name.


Source distributions and wheels are built with `uv build`, including native
manylinux builds inside the platform containers. Publication uses `uv publish`
with the existing trusted publisher and PEP 740 attestations for the original
tested archives. Normal development uses `uv sync --locked --extra test` and
`uv run --locked --extra test`; no pip interface or tox environment is used.
