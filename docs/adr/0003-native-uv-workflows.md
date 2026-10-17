# Native uv project workflows

Status: Accepted

The repository uses native uv commands for development, test execution, wheel
and source-distribution builds, and publication. The pip interface, including
uv pip, and tox are not used. Python tools run through uv; development tool
versions are resolved in the committed uv.lock. Dependabot maintains uv locks.

Normal setup uses uv sync --locked with the test extra. Ruff and pytest use
uv run --locked; mypy is available in the development group. This migration does
not silently introduce a passing type-check baseline or suppress typing errors.

Coverage and wheel tests create separate uv projects, explicitly pin the requested
Python interpreter, add the built wheel with uv add and sync their locks. The
native-coverage runner still verifies loaded extension bytes against the exact
instrumented wheel, requires actual Python-to-Rust execution, and checks the
complete owned-source denominator. It uses uv build with Cargo's dev profile for
instrumentation. Coverage requirements and Python/Rust package contracts remain
unchanged. Legacy requirements snapshots are generated with uv export and imported
into isolated test projects with uv add; uv.lock is their source of dependency data.

Native release wheels are built from the verified source archive with uv build,
including manylinux builds inside the existing platform images. Publication uses
uv publish with the existing trusted publisher, original tested archives, registry
hash verification and PEP 740 attestations. Artifacts are never rebuilt in the
publishing job. Version preparation must update uv.lock alongside pyproject.toml.

References:
- https://docs.astral.sh/uv/concepts/projects/sync/
- https://docs.astral.sh/uv/guides/publish/

The manylinux container mounts only the extracted source archive, target cache,
wheel output and a read-only build driver. It does not mount the parent checkout.
Cargo searches ancestor directories for configuration; exposing the checkout
would leak repository-only hook opt-ins into dependency builds from the archive.
