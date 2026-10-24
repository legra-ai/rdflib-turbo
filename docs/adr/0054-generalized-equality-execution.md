# ADR 0054: Execute shared equality across generalized tuples

Status: Accepted

Extends ADR 0053's generalized shared-rule execution and ADR 0036's ordinary
indexed equality kernel.

## Defect

The ordinary kernel cannot retain literal subjects or non-IRI predicates. When
two named terms are each `owl:sameAs` the same literal, omitting the inverse
literal-subject edges loses the named terms' equality and ordinary replacement
consequences. A retained native regression fails on the merged prior runtime.

## Decision

Use the eight published `rdf-reasoning` equality clauses as the semantic source
for the missing tuple domain. Partition their derivations by RDF representability:

- The existing indexed kernel and ordinary reflexivity/predicate queries execute
  RDF-only premises with ordinary RDF heads.
- Four clauses can produce a non-RDF head from RDF-only premises: object
  reflexivity, symmetry, subject replacement and predicate replacement. Lower
  those clauses with a head-kind filter and evaluate them on ordinary storage.
- For every positive premise in all eight clauses, lower one query that requires
  that premise to come from the retained generalized relation. Its other premises
  read the ordinary/generalized union. This covers every derivation with a
  non-RDF premise, including those producing ordinary heads. Multiple witnesses
  still produce one pending tuple.

An internal named-graph query view identifies the generalized premise. It is a
capability of this inference-local evaluator, not a caller graph or persistent
storage schema. The view exposes the actual tuple terms and never exposes the
physical row identities or column predicates used by the native relation.

All three execution parts read one immutable round snapshot. Ordinary and
generalized results advance at the existing common publication boundary.
New inverse edges cannot become premises during the same round. Existing scope,
source-error propagation, destination suppression, transactions and rollback
remain in force.

Preserve exact lexical/blank-node/term identity in joins. Explicit `owl:sameAs`
replacement is distinct from datatype value equality; this change supplies no
new lexical conversion, datatype membership or implicit numeric equality.

The ordinary kernel retains its existing indexed algorithms. Generalized
execution does not route ordinary closed equality cliques through unrestricted
replacement queries. Query tables are finite: four head-filtered queries and
twelve generalized-premise queries. The generalized materialized relation retains
the existing memory contract from ADR 0053; arbitrary closure size is not bounded.

## Evidence and remaining scope

Compare the partition's one-round output with unrestricted shared clauses across
all 512 directed equality graphs on an IRI, a blank node and a literal, together
with data tuples using literal subjects and a blank predicate. Keep round-boundary,
exact lexical identity, scoped graph, failure and allocation controls. Compare
complete ordinary closure projections with the pinned Python reference through
installed source wheels, including transactional and persistent stores.

This covers equality derivations over the current generalized term domain.
Specialized list/cardinality operations, datatype semantics, inconsistency
diagnostics and full Python extension compatibility remain separate work. These
tests do not certify complete OWL-RL/RDF conformance.
