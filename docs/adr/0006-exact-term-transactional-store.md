# ADR 0006: Preserve RDF terms before adopting a transactional Store

Status: Accepted; storage prerequisite implemented, runtime adoption pending

## Decision

Embed Oxigraph in this native extension as the transaction-capable storage
foundation. Keep in-memory operation as the default and make RocksDB persistence
optional. The canonical SHACL engine remains a Rust dependency; owlrl-turbo and
pyshacl-turbo remain Python compatibility facades. Shared RDF, OWL, SHACL, Datalog
and rule-model crates remain independent of this backend.

This supersedes the choice of oxrdf::Dataset as the long-term mutable data owner,
not ADR 0004's single-extension ownership or ADR 0005's current inference behavior.
Do not switch the public Dataset until inference and SHACL can borrow the store
or an active transaction directly. Copying a persistent store into an in-memory
Dataset is not an implementation of the shared-store design.

## Storage prerequisite

Upstream Oxigraph 0.5.11 canonicalizes recognized typed literals in its numerical
storage encoding. Distinct RDF terms such as "01"^^xsd:integer and "1"^^xsd:integer
collapse; decimal spelling and derived datatype identity also disappear. A query
adapter cannot recover information that storage has discarded.

Vendor the published crate with its archive checksum, source revision and both
upstream licenses. An opt-in exact-term-storage feature uses the existing generic
typed-literal encoding for explicitly typed literals. It enables the existing
plain-versus-explicit-string identity feature and the evaluator's exact-term
builtins. Do not add a side table, second dataset or new storage format version.
Without the feature, upstream encoding and evaluation remain unchanged.

Keep numeric value semantics by decoding expression values for comparisons,
aggregates and effective boolean value. Intern computed expression results through
the same RDF-term encoding as inserted data, so joins compare consistent keys.
STR and DATATYPE inspect the original RDF term rather than a parsed numeric value.
Their opt-in evaluator path also applies to the existing Dataset runtime.

The isolated tests/store-contract crate exercises this storage contract without
Python or the SHACL engine. It checks default in-memory operation, RDF 1.2 feature
compilation, RocksDB reopen/abort, and feature-off upstream behavior. Runtime tests
also exercise the shared SPARQL evaluator. Native tests are not a claim that the
public Python Dataset already supports Store transactions or persistence.

## Remaining integration contract

Expose one explicit transaction owner, with queries, updates, inference and SHACL
reading its uncommitted changes. Commit only after the requested operation and
report construction succeed. On errors or unwinding, drop the transaction to abort;
never snapshot the original dataset for rollback. Validation nonconformance is a
result, not an execution error; define its commit policy explicitly at the API.
Keep graph scopes, inferred-fact destination and namespace metadata consistent
with the same operation boundary. Do not share Rust pointers between extensions.

Oxigraph buffers transaction changes in memory. This removes rollback copies of
the base dataset; it does not promise bounded memory for an arbitrarily large
transaction, every SPARQL query, inference closure or validation report. Measure
actual installed wheels, base-store size, transaction delta and process RSS.
Public persistence, transaction APIs and direct engine adapters remain follow-up
work and must have their own regression and installed-wheel tests before release.
