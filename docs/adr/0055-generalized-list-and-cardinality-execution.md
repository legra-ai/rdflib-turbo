# ADR 0055: Preserve generalized list and cardinality consequences

Status: Accepted

Extends ADRs 0008, 0034, 0053 and 0054.

## Defect

The specialized property-chain, intersection-recognition and key kernels read
only ordinary RDF. They cannot follow a chain through a literal, recognize a
literal instance of an intersection, or join a literal instance on a key.
The three maximum-cardinality-one queries also lose literal-subject equality
heads through CONSTRUCT projection. Each omission can lose ordinary RDF output;
four retained regressions fail against the merged previous runtime and their
expected consequences are present in the pinned Python owlrl reference.

## Decision

Give the specialized kernels indexed tuple access to the immutable union of the
ordinary backend and the existing call-local generalized relation. Bound
ordinary lookups still use the backend indexes and boolean membership boundary;
non-RDF selectors consult only the generalized indexes. Stream candidates rather
than collecting all instances or copying the input graph. Intersection traversal
retains its reachability semantics and indexed member seed. Strict property-chain
and key list validation retains its existing finite, unambiguous IRI-property
list contract; this is not a new permissive list parser.

Route kernel heads through the same ordinary/generalized classification as
shared SELECT results. Both kinds remain pending until every rule finishes the
round. No traversal result is immediately visible to another rule or path.
Existing transaction scopes, output destinations, storage errors and rollback
remain authoritative.

Lower the existing three cardinality-one patterns to SELECT head bindings,
retaining the existing numeric cardinality filter and qualified-class/Thing
branches. Evaluate them against the same union as the portable clauses. This
changes head retention and premise visibility, not the source or interpretation
of cardinality definitions. The portable positive-list catalogue remains the
source for the five already shared list clauses. No dependency change or
additional rule catalogue is introduced.

## Boundary

This corrects the demonstrated generalized instance/chain/cardinality defects.
It does not implement a complete generalized schema language, replace strict
chain/key list validation, add datatype value equivalence, or generalize
inconsistency diagnostics. Full OWL-RL/RDF and Python extension conformance
remain separate claims requiring separate evidence.
