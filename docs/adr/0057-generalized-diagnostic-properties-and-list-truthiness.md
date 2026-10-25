# ADR 0057: Diagnose blank-node properties and traverse literal list cells

Status: Accepted

Extends ADRs 0040 and 0056.

## Evidence

Combined main after PRs 83 and 84 passes 694 installed-wheel cases, including
the shared-engine single-line integration cases, and all 18 unchanged RELEASE
allocation budgets. Independent probes against that wheel then demonstrate
six missing blank-predicate diagnostics and six missing truthy literal-list
cases. Empty, zero and false list controls already pass.

## Decision

Allow exact term selectors in the existing diagnostic tuple view. Property
collectors preserve every asserted property term, including literal predicates
already present in a programmatically populated graph. The pinned Python
reasoner diagnoses those facts. Preserve its deliberate rejection of derived
literal predicates at the pending-publication boundary: Core.store_triple
excludes them. A blanket literal-property guard in the collectors would wrongly
suppress diagnostics for asserted facts. Do not treat that compatibility boundary as
proof of standard OWL-RL/RDF generalized-domain completeness.

Traverse literal-valued diagnostic list cells using the default RDFLib scalar
truth-value profile, rather than rejecting their term kind or using lexical
nonemptiness alone. Reuse existing integer, float and Decimal parsing primitives.
Decimal truth checks are quiet, including signaling NaN; they do not perform
cardinality comparison or raise comparison flags. Binary emptiness and duration
rounding have their own scalar checks. Frozen differential cases cover the
pinned default converter profile. Unknown or ill-typed values follow the
reference Literal.__bool__ lexical-length contract, not the separate Alt lexical
validation profile. The existing cached Python Literal.value transport gap is
unchanged; this does not claim preservation of values absent from exact terms.

Pass the existing immutable per-invocation context through list cursors.
Preserve first-edge witnesses, per-round snapshots, cycle errors and reference
last-member quirks. No Python reasoner, graph copy, dependency, persistence shape,
public API or standard-profile preset is introduced. Asserted generalized facts
remain intact and diagnostics publish only after successful closure.

## Remaining semantic foundation work

Portable positive clauses remain sourced from rdf-reasoning. These diagnostic
checks implement the versioned Python compatibility adapter; they do not add an
independent positive-rule catalogue. Shared standard datatype value operations,
RDFS non-string substitutions, cached-value transport, and a complete
rule-by-rule OWL-RL conformance inventory remain separate work. Keep standard
profile coverage distinct from Python compatibility evidence.
