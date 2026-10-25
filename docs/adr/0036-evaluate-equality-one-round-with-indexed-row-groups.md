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
witnesses still reproduce existing facts. Discover weak connectivity only when
the existing row signatures identify an unbalanced group: its outgoing degree
exceeds its number of identical-row subjects. Balanced groups need no component
index. Their repeated co-neighbor witnesses and already-present transitive
witnesses are bounded by their input edges; missing transitive witnesses are
bounded by their distinct output edges. Keep each row group members-only and
leave dense flags/component storage unallocated on balanced-only inputs.

Explore an unbalanced seed through indexed outgoing and incoming reads, retaining
at most twice its degree plus one overflow term. A larger component proves that
the seed cannot have a majority outgoing row; discard that partial component and
resolve encountered seeds of no greater degree. A completed small component
classifies every row group it contains, including balanced groups. Retain its
term identities for witnessed candidate search, and reuse it for the other seeds.
Unrelated balanced components are never explored. A completed bounded component
is also reused by its unbalanced row groups without a strict-majority gate.
Balanced sparse rows retain witness enumeration; balanced majority rows in the
same component retain their candidate-search route. These components only
restrict the candidate search space; they never establish equality or make
pending inverses visible. Do not retain an equality-edge copy or a global
connectivity index.

For a selected row with a completed bounded component, examine only missing
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

Resource regressions require zero component allocation/connectivity reads for
closed cliques and disjoint closed pairs. Mixed inputs retain only the small
nearly closed component. A degree-two seed in a large incoming star must stop
before scanning the full component. These contracts complement the work bounds
and exact one-round differential tests.

The strict-majority gate discarded already completed searches at half-connected
rounds. A deterministic 64-node circulant regression required 177,346 indexed
reads/probes against a fixed 131,072 budget; merely deduplicating co-neighbor
heads still required 149,762. Reuse those completed candidate domains for
unbalanced rows and prove each missing pair against the original snapshot.
The 64- and 128-node regressions retain their fixed `32 * nodes²` work budgets.
This does not change eager closure, exploration limits or round publication.

The original sparse/balanced enumeration route remains unchanged. Dense/mixed,
exact one-round, closed-pair/clique allocation and incoming-star regressions
continue to define the acceptance boundary. This remains an adaptive work-saving
strategy, not a universal quadratic complexity claim.

Sparse inverse compositions now stream common predecessors per output row.
The current outgoing term set rejects already-present targets; the same
missing-target delta is reused after grouped transitive candidates have been
emitted. Identical predecessor rows contribute through their actual incoming
representative once. Terminal named or blank subjects without outgoing rows
retain the source-group witness route; literal targets remain objects only.
Only source scans establish witnesses. Final pending publication still checks
source plus destination membership, retaining separate-destination suppression.
No second outgoing set, additional connectivity index or edge copy is retained.

Actual 128-node ring composition reached 94,208 existing-head storage probes in
one sparse round, including 90,112 repeated existing heads. A regression requires
zero existing-head probes during composition for every 64- and 128-node ring
round. This removes redundant exact storage probes, not all repeated witness
scans; it does not establish a universal subcubic work bound. The existing fixed
work/resource bounds and immutable-round differential oracles remain unchanged.

The output-row route must not add one incoming query per isolated reflexive
subject or closed-row member. During the existing grouped transitive pass,
annotate each existing subject slot when the representative outgoing row proves
an incoming edge from a different exact row group. Mark even dense source rows.
After every group has contributed, evaluate inverse compositions in a second
phase. Skip groups without annotated members before loading another row: every
predecessor has exactly the current outgoing targets, so no head can be new.
Terminal subjects retain their source-group route. Both phases read only the
original source snapshot, never pending or destination-only edges.

Use a checked, typed one-word subject slot: the row index occupies its high bits
and the cross-group flag its low bit. A physical row-group Vec already restricts
its indices sufficiently; overflow still fails explicitly. Subject-map values
remain exactly one usize and row groups remain exactly one member Vec. No new
map, relation or outgoing-row set is retained. Regressions preserve those layouts,
require only the original preparation source scan, and bound complete
composition of disjoint self, pair and clique groups to one representative
outgoing read per group with zero incoming queries. Active groups load one
current row for the inverse phase; previous-phase rows and deltas have already
been dropped. Existing work and allocation budgets remain.

## Full-Term reader extension

ADR 0065's generalized internal inference owner supersedes this decision's earlier
restriction that literal terms cannot be subjects or outgoing intermediates.
Row-group members and subject identities now retain full terms, including exact
literal datatype/language state. Grouping, witnesses and pending publication still
read one immutable round; no pending inverse becomes visible prematurely.

The former strict CONSTRUCT output remains an ordinary-domain control. For
mixed-role inputs it cannot serve as a complete oracle because it omits literal
subjects. Independent naive immutable full-Term algebra supplies the exact
one-round reference, anchored against an actual synthetic OWL 7.6.2 equality round
with literal subjects and blank predicates before cleanup. This changes neither
generic public CONSTRUCT projection nor the separate publication policy.
