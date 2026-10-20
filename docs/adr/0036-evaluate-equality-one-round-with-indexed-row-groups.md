# ADR 0036: Evaluate equality one round with indexed row groups

Status: Accepted

The OWL equality rules repeatedly enumerate transitive and replacement witnesses
whose triples already exist. A closed equality clique has quadratically many
facts but cubically many intermediate witnesses. Filtering equality predicates
inside replacement queries still allows lateral scans to visit that cubic bag.

Evaluate symmetry, transitivity and subject/object replacement directly against
the immutable current-round graph. Stream ordinary quads and reject the equality
predicate before alias lookups; retain predicate substitution as its existing
query. Exact outgoing-row groups allow equality compositions to share work.

Build transient subject-to-group identities and group members, retaining only one
outgoing term set at a time. Row signatures are order independent; candidate
representatives are checked by exact membership and distinct row cardinality.
Signature collisions never establish equivalence. Union graph duplicates do not
change row identity. During composition retain one row and its missing-target
delta, never all outgoing fact sets or a graph clone.

The kernel computes only the former rules' one-step consequences: the inverse
relation, the relation composed with itself, and inverse-relation composed with
the relation. The last composition preserves equality-predicate subject
replacement, which is not implied by same-round transitivity because newly
inferred inverse edges remain pending. Literal objects remain exact terms;
literals cannot become subjects or intermediates with outgoing edges.

Every read uses the original round snapshot and every result enters the existing
pending additions set. Publish additions only after all ordinary rules, list
rules and keys finish. Do not replace this with eager connected-component closure:
nonmonotone list filters make intermediate rounds observable (ADR 0005).

A synthetic closed-clique regression measures evaluator intermediate rows and
indexed read results, with a quadratic work bound and no timing threshold.
Exhaustive tiny graphs compare one-step output against the former frozen rules,
including blank nodes, literal edges and predicate aliases. Chain/fork tests
protect round boundaries and co-neighbor consequences; complete fixed points
also match the frozen ordinary rule engine. Existing transaction rollback and
graph-scope contracts remain unchanged.

Identical-row grouping alone does not bound work for a nearly complete relation:
missing diagonals make every outgoing row distinct while most composition
witnesses still reproduce existing facts. Track weak connectivity with transient
term identities and a union-by-size forest. These components only restrict the
candidate search space; they never establish equality or make pending inverses
visible. Retain O(distinct equality terms) component metadata, not an edge copy.

For a row containing more than half its component's terms, examine only missing
targets. Establish each addition with an indexed two-edge transitive witness or
a common predecessor from the original snapshot. Transitive witnesses are shared
by identical outgoing rows; common-predecessor witnesses remain per subject.
Keep sparse rows on the witness-enumeration path, including sparse output rows
reached as co-neighbors of a dense source row. Literal terms can be candidate
objects but never outgoing intermediates. This adaptive choice is a work-saving
strategy, not a universal quadratic complexity guarantee.

Missing-diagonal single and disjoint cliques count both indexed read calls and
emitted rows, including unsuccessful exact probes. Their quadratic synthetic
budgets remain fixed. Mixed dense/sparse graphs, literals, blank nodes and
disconnected components also match the frozen one-round rule set. Storage errors
during dense probes preserve pending-publication and insertion-journal rollback
contracts.
