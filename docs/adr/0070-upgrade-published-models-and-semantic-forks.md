# ADR 0070: Upgrade published models without discarding semantic forks

Status: Accepted for source integration; fresh installed-artifact validation pending

Consume datalog-structural-model 3.0.0, rdf-reasoning 2.0.0 and
shacl-structural-model 6.2.0 through the existing typed adapters. The SHACL
model introduces memberShape, minListLength, maxListLength and uniqueMembers.
Preserve the already public companion implementation from pyshacl-turbo
commit b86f06162ab252e178919c5d984ab0f0b3a0c259 in the provider-owned engine,
including its borrowed-list regressions. Adapt its reads to the provider's
full-term GraphRead and indexed shapes source; do not restore a second owner.

List structure is checked before members, with constant cursor state and
arbitrary-precision counters. Uniqueness uses pairwise exact-term probes and
has quadratic worst-case time. False uniqueness still requires a valid list.
Reports retain list-head identity instead of expanding the entire collection.
Length parameters use strict non-negative XSD integer lexical syntax without
changing existing Core integer parsing. These selected draft constraints do
not establish complete SHACL 1.2 conformance or original pySHACL parity.

Upgrade the published Oxigraph dependency family coherently. Three-way merge
each original old archive, the local semantic fork and the original new archive.
Retain local-only modules and exact-term, generalized, transaction, scoped read,
query-profile and host-authority behavior. A published dependency update does
not authorize replacing the fork with an unmodified registry crate.

Keep original archive checksums and upstream revisions separate from patched
source checksums. Preserve historical entries and license bytes, update active
paths, standalone lockfiles, source collectors and current cohort tooling, and
run both upstream and patched feature profiles. Historical measured input
cohorts and their tool versions remain immutable. New source identities require
fresh artifacts; previous wheel, coverage and performance receipts do not
attest upgraded code. Publication remains restricted to the tested CI pipeline.
