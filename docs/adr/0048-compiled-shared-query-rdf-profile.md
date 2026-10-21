# ADR 0048: Attest the compiled shared query and RDF profile

Status: accepted.

## Decision

Store validation, ordered query/UPDATE contexts and owned prepared algebra use
one canonical engine source. Native Cargo, the isolated Python companion and
fancy-regex select the same immutable Git revision. The reported source revision
comes from the typed native Cargo manifest during compilation; a missing or
mutable source pin fails the build. It is not a manually synchronized constant.

The provider vendors byte-identical implementations and manifests for the six
query/RDF dependencies compiled by that engine: oxigraph, oxrdf, oxsdatatypes,
spargebra, sparopt and spareval. Each compiled crate reports its own source and
effective-feature digest. The engine combines those actual digests with its
implementation identity. Hashing an unused canonical vendor directory cannot
attest a substituted provider dependency. The shared build helper is itself an
identity input and ships in source archives.

Features are semantic by default. Only Oxigraph's `rocksdb`, `rocksdb-debug` and
`rocksdb-pkg-config` selections have a separate audited backend-capability digest;
they do not change the borrowed GraphRead query/RDF contract. The optional native
dependency uses explicit `dep:` activation so it cannot leak another semantic
feature. Exact-term storage, RDF versions and every unknown feature remain in
the semantic identity. This separation requires real memory/persistent borrowed
execution tests; it does not attest the external backend implementation.

The shared profile preserves indexed borrowed reads, exact term identity,
scoped callbacks, report acceptance and ordered query/UPDATE context. Prepared
plans retain their original expression scope and reject a mismatching source
tree or UPDATE use. It adds no dataset snapshot, Python evaluator fallback or
protocol negotiation. Protocol remains 1. Source/profile mismatches fail before
execution. Successful report consumption still controls staged write acceptance.

## Verification

The build-metadata harness rejects missing, malformed and mutable pins and checks
that unrelated metadata cannot override the declared source. CI and `make test`
run it. Installed companion tests compare the actual engine identities; existing
scope, lexical-term, prepared-plan, warning/decoding rollback and transaction
mutation-attempt regressions remain required. Package audits check the actual
build script and helper bytes, including newly introduced source modules.
