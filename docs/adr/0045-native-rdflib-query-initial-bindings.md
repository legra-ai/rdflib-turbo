# ADR 0045: Evaluate RDFLib initial query bindings in native algebra

Status: Implemented; installed-wheel query and coverage verification passed

RDFLib initial bindings are an initial query context, rather than textual
substitutions or a VALUES clause. They can bind variables hidden by a nested
projection; an unused binding does not extend SELECT output; BIND can replace
a context binding. Strict SEP-0007 substitutions have different scope and
conflict rules and keep their existing API.

Graph and StoreDataset queries select this context mode even for an empty
parameter mapping. The lower-level Store query API retains ordinary SPARQL
semantics unless the caller explicitly supplies the RDFLib context.

Transport parameter terms through the existing typed exact-term boundary.
Never interpolate values into query text or invoke RDFLib's Python evaluator.
Execute against the caller's selected graph, union dataset or pending native
transaction. Invalid parameters abort an explicit transaction even when the
caller catches the error.

The explicit evaluator initial-context mode lowers original parsed algebra
with RDFLib's lazy-join eligibility and OPTIONAL retry variable scope. Preserve
that metadata before lowering basic graph patterns: a basic graph pattern's
internal joins differ from original nested Join, DISTINCT and Slice barriers.
EXISTS retains its original unsimplified group wrappers through an explicit
parser mode. Its joins evaluate independently, matching RDFLib's separate
translation path; its OPTIONAL expressions retain original source scope.
Ordinary SPARQL parsing and preparation remain unchanged.

RDFLib-mode lazy joins stream the correlated right side, restore global initial
bindings when creating its context, and merge the left row with RDFLib's
precedence. OPTIONAL streams matches and retries with the original source
variable scope. MINUS keeps its independently evaluated sides and retains
global binding domains. Each projection restricts its explicit output domain,
including DESCRIBE's selected resources. Independent joins check compatibility
directionally: iterate the left explicit domain and look up each value in the
right row, consulting initial bindings for absent keys. That lookup must not
insert keys into the right row, because MINUS observes the explicit domain.
Independent joins materialize a distinct right-hand solution set and stream
left-hand rows, preserving RDFLib's duplicate behavior at these barriers.

Do not apply the ordinary SPARQL optimizer to this order-sensitive algebra.
Its join reordering and projection assumptions are invalid for these context
semantics. Optimize only individual BGP leaves to select indexed hash-join
keys without reordering their enclosing groups. Explicitly disabling
optimizations suppresses this leaf optimization too.
The native indexed dataset adapter remains the execution backend;
this does not copy the input relation. Query-plan caching for ordinary SPARQL
continues to use the existing bounded cache. Parameter values are never cached.

Verify native algebra regressions, paired original-RDFLib query results,
projection and barrier behavior, exact terms, selected contexts, transaction
read-your-writes and rollback, and the installed wheel's Python coverage.
Prepared Python query objects and initial SPARQL Update bindings remain
separate unsupported contracts.
