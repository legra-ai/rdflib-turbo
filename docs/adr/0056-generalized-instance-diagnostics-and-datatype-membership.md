# ADR 0056: Check generalized instances and retain literal datatype membership

Status: Accepted

Extends ADRs 0040, 0053, 0054 and 0055.

## Defects

The diagnostic collectors read ordinary RDF even though entailment now retains
literal-subject intermediate tuples. Disjoint classes, Nothing, asymmetric
properties and negative property assertions consequently miss literal instances.
Separately, OWL initialization records used literal datatypes but omits their
literal-owned type and ancestor-type tuples. These omissions can suppress both
inconsistency reports and ordinary RDF consequences. Retained native tests fail
on the prior merged runtime; the pinned Python owlrl reference confirms their
expected results.

## Decision

Reuse the indexed tuple view from ADR 0055 for diagnostic relation, restriction
and list-instance reads. Preserve per-round list witnesses and the reference's
list comparison quirks. Literal individuals participate in the same exact-term
joins as named and blank individuals. Keep messages and diagnostic-node
publication at the existing final success boundary. Physical generalized row
identities never enter messages or public output. Backend errors remain hard
execution failures with the existing rollback behavior.

During the existing OWL datatype initialization scan, retain recognized
literal-owned rdf:type tuples and their datatype ancestors in the call-local
generalized relation. These are initial axioms, visible before the first round,
just like ordinary initialization output. Reuse the existing recognized datatype
inventory, lexical converter, ancestor hierarchy and transaction ownership.
RDFS-only initialization remains separate.

Literal-subject rdf:type tuples whose class is an IRI or blank node contain two
independent values. Store a single inverse record in the existing private
indexed relation and reconstruct the logical triple on reads. Preserve both
bound-slot indexes, exact lexical identity and ordinary/generalized output
classification. Other type shapes retain the general representation. This
removes redundant generated row identities and column indexes; it does not
remove facts or relax the existing inference heap budget. At round publication,
adopt the pending relation directly when the current relation is empty; otherwise
release each pending record's indexes before inserting it into the current
relation. Do not retain both complete index sets during transfer.

## Scope

This corrects generalized *instance* diagnostics and recognized literal datatype
membership. Existing IRI-property checks and non-literal diagnostic list-cell
contracts remain. This does not claim complete generalized schema/list-node
truthiness support or full OWL-RL/RDF conformance.

The pinned Python OWL reasoner deliberately skips dt-eq and dt-diff; do not add
implicit numeric value equality as a compatibility fix. Exact RDF term identity
continues to govern joins. ADR 0041's pending RDFS numeric and other value
profiles require separate implementation and reference evidence; this decision
does not broaden its string substitution profile or resolve cached Python
Literal.value transport.
