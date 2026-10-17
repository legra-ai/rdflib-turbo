# Local patch: optional RDFLib string term identity

Upstream: Oxigraph oxrdf 0.3.4, Apache-2.0/MIT, `lib/oxrdf` at
`0e81a29d27575ab69e7f85ea9c7f1058b277fcd0`. Original licenses retained.
The Cargo registry source is pinned in the workspace lockfile; this directory
replaces that version through `[patch.crates-io]`.

The opt-in `rdf-10-string-identity` feature preserves the existing String and
TypedLiteral variants when the explicit datatype is xsd:string. Constructors,
borrowed literals and Dataset interning retain the distinction. The new
`has_explicit_datatype` accessor exposes it without inventing private datatypes.
Datatype access still returns xsd:string for both forms. Without the feature,
the upstream RDF 1.1 identity behavior is unchanged.

pySHACL's RDFLib inputs distinguish plain strings from explicit xsd:string in
Core term comparisons. The engine enables the feature for that compatibility;
SPARQL value equality continues to treat both as equal, while sameTerm and
Dataset membership preserve their identities. Parsing and N-Quads serialization
retain explicit types. PyOxigraph Store input is adapted separately: its API
exposes all strings as explicit xsd:string and has already canonicalized terms.

Regressions: `tests/test_literal_identity.py`, `tests/test_dash_literals.py`,
`crates/engine/tests/pipeline.rs::plain_and_typed_strings_survive_dataset_round_trip`,
and `literal::tests::test_explicit_string_identity`. The unmodified upstream
simple-literal equality test remains active when the feature is disabled.

## Additive dataset transactions

`Dataset::insert_transaction` journals only new compact interned quad keys and
new interner entries. Its read-only dataset view and explicit insert/extend API
prevent unjournaled removals or clears. Commit retains insertions; guard drop
rolls back all indexes and newly interned terms, including during unwinding.
Snapshots cloned from its read view are independent. This local API is used by
native inference to preserve atomic failure without copying the existing six
indexes. See `dataset_transaction_tests.rs` and runtime ADR 0005. Original crate
archive hashes and licenses remain unchanged as source provenance.
