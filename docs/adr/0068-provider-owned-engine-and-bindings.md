# ADR 0068: Own the engine and bindings in the native provider

Status: Accepted for source consolidation; installed artifact validation pending

Supersedes ADR 0004's revision-pinned external engine build and independently
built engine pairing. Dataset ownership, transaction acceptance, warning policy
and exact-term semantics remain binding.

The existing rdflib-turbo extension owns the reusable SHACL engine and both
binding surfaces. Keep the engine and its Rust tests in `crates/engine`, and the
SHACL binding library in `crates/python`. Both are local workspace dependencies.
The binding library is an rlib, with no standalone extension or ABI3 feature.
There is one extension and the CPython-specific wheel contract from ADR 0020
continues to apply. A fourth Python distribution is not required for ownership.

Keep the semantic fork sources and feature profiles in this repository. Native
coverage measures the provider, engine and binding implementation files through
the existing owned-source collector. Preserve declaration-only exclusions only
with their exact source hashes and missing-executable-mapping checks. Do not
omit moved code to preserve the old percentage. Fork implementation measurement
remains a separate explicitly scoped obligation.

The engine fingerprint continues to cover its real source and compiled semantic
fork identities. The native source declaration identifies `crates/engine`;
there is no external Git revision to report. An installed facade aliasing this
extension cannot independently attest another engine. Verify actual built
source archives, wheel bytes, feature profiles and shared class/program identity
instead of treating equal aliases as independent provenance.

Thin Python facades must exercise installed provider artifacts. Release the
provider through its existing tested publisher before changing public facade
requirements to a registry version with the necessary verified capabilities.
Do not invent a future minimum version or add another native publisher. The
prototype's development sources and packaging workflows still require separate
promotion and artifact validation before publication.

This changes source ownership, not algorithms. Generalized shape identifiers,
report parity, cache retention and performance require their own regressions
and measurements; relocation does not establish those claims.
