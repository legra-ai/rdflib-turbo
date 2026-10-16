# ADR 0001: Own one native dataset runtime

Status: Accepted for the first alpha

rdflib-turbo owns the native exact-term Dataset and its parsing, serialization,
SPARQL and inference operations. owlrl-turbo depends on the public Dataset API,
so native-to-native reasoning uses the same object without RDF text exchange.
The inference implementation is extracted from pyshacl-turbo; its provenance
is recorded in vendor/README.md. pyshacl-turbo integration initially uses explicit
RDFLib conversion. It does not yet share this runtime.

Preserve literal lexical forms, datatype identity and blank-node links. Ordinary
documents have separate blank-node scopes. Explicit RDFLib conversion preserves
caller identities. Default graphs and union views are distinct. Parsing and
inference stage all changes and commit only on success. Unsupported operations
raise errors; no Python query or inference fallback exists. Public inferred RDF
never contains literal subjects or non-IRI predicates. The pinned reasoner's
known incomplete owl:hasKey comparison is intentionally not reproduced.

These are narrowly scoped functional alphas, not complete drop-in replacements.
Neither compatibility percentages nor performance gains are claimed. Future
SHACL integration must resolve dataset ownership, transaction and cache-version
contracts before claiming a shared native pipeline.
