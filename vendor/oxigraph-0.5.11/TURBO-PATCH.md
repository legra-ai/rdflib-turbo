# Local patches to Oxigraph 0.5.11

Original published archive SHA-256:
`7f6632dd080bd07676f06e70254ff5862c9943e9483d700fa7258df73a3f9a6a`.
Original source revision: `df37a5c98e2497135cdd4cfce01a049b78ca6740`.
MIT and Apache-2.0 license texts are retained from upstream v0.5.11.

The opt-in `exact-term-storage` feature stores explicitly typed literals with the
existing generic typed-literal encoding. Lexical spelling, declared datatype and
plain versus explicit xsd:string identity survive insert, query, update and
transaction commit. No additional indexes or side tables are introduced.

With the feature enabled, the dataset adapter uses QueryableDataset's RDF-term
internalization and decoded effective-boolean-value methods instead of the
upstream native numeric shortcuts. Value expressions still parse numeric values;
computed RDF terms use the same encoding as stored RDF terms. The feature also
enables spareval's local `exact-term-builtins` feature for STR and DATATYPE.
Without the opt-in, upstream encoding and shortcuts remain unchanged.

See ADR 0006 and `tests/store-contract` for the feature matrix and persistence
regressions. This dependency patch does not itself expose a Python Store API.
