# ADR 0053: Retain generalized heads from portable reasoning clauses

Status: Accepted for shared relational and positive list clauses

## Defect

Lowering portable Datalog clauses to SPARQL CONSTRUCT projects away literal
subjects and non-IRI predicates before the next reasoning round. Standard RDF
output can consequently lose valid consequences. For example, inverse property,
subproperty and inverse property derivations can pass through literal-subject
tuples and return to an ordinary triple. The retained four-statement regression
fails on Dataset, Store and explicit transactions in release 0.2.0a14.

## Decision

Lower the existing shared clause selection to SELECT bindings for all three head
positions. Preserve exact term identity, including numeric lexical distinctions.
Ordinary heads use the existing pending output and transaction owner. Non-RDF
heads use a call-local indexed relation that is never published in the caller's
dataset. Keep prior-round generalized facts immutable while evaluating every
clause; publish their delta only at the same round boundary as ordinary facts.
Terminate only when both deltas are empty.

The backend represents each generalized row with three indexed column values in
its existing native Dataset. A query view borrows ordinary storage and joins it
with those rows using unrestricted SPARQL internal terms. No generated row
identity or column predicate is visible to reasoning queries or public output.
Retain only non-RDF intermediate facts; do not copy the asserted graph. Lookup
selectors are transient and independent of relation size. While no intermediate
rows exist, use the backend's existing borrowed query implementation.

The relation is inference working state, not a persistent schema or public
serialization format. It is dropped on completion or failure. Existing native
transactions and insertion journals still govern ordinary publication and
rollback. This change does not make arbitrary materialization memory bounded:
the native indexed relation retains the non-RDF intermediate closure, just as
the existing pending output retains ordinary additions.

## Coverage boundary

The selected shared clauses now retain and consume generalized tuples. This does
not replace the specialized equality, intersection-recognition, property-chain,
key, datatype or diagnostic kernels. Their complete generalized-domain behavior,
full OWL-RL/RDF conformance and Python extension compatibility remain separate
work. Do not expose a complete standard profile on this evidence.

Compare the full ordinary projection of the regression closures with the pinned
Python reference, using graph isomorphism for blank nodes. Exercise exact identity
and indexed generalized row lookup natively, and retain the existing transaction,
scope, failure-injection and rule-adapter suites.
