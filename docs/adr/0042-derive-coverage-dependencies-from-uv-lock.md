# ADR 0042: Derive measurement dependencies from the authoritative uv lock

Status: Accepted

Supersedes ADR 0003's legacy requirements snapshot choice. Its native uv,
isolated test project, exact installed wheel and owned-source measurement
requirements remain binding.

Committed requirements exports duplicate dependency inputs and can remain stale
when Dependabot updates pyproject.toml and uv.lock. The runtime no longer commits
or distributes requirements-coverage.in or requirements-coverage.lock. The
coverage group in pyproject.toml and its resolved uv.lock are authoritative.

Before creating the isolated measurement environment, run uv export with
--locked, the explicit source project, --group coverage, --no-dev,
--no-emit-project and --no-hashes. Write the generated requirements to an absolute
per-measurement profiles path. Import that export using uv add --no-sync, then
uv sync --locked in the isolated project. Failed locked export aborts; never
install a stale snapshot, resolve unlocked source dependencies, modify the source
project environment or substitute an editable source build.

Preserve the source pyproject.toml and uv.lock, generated export, uv version and
selection flags alongside the report. Verify their hashes after setup and before
publishing measurement output. Preserve each Rust, Python and companion execution
stage's actual environment lock separately because installing an instrumented
runtime or companion wheel changes that lock. Record the companion wheel hash
and revision as well as the instrumented wheel and loaded extension hashes.

Source archives include uv.lock and the runner. Consumers pinned to older source
commits retain those commits' files; new source archives have one dependency
lock authority. Cross-repository runners and immutable workflow source pins must
adopt this decision through their own coordinated changes.
