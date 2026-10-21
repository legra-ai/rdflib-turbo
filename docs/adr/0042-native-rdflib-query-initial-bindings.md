# ADR 0042: Evaluate RDFLib initial query bindings in native algebra

Status: Implemented; awaiting installed-wheel verification

RDFLib initial bindings are an initial query context, rather than textual
substitutions or a VALUES clause. They can bind variables hidden by a nested
projection; an unused binding does not extend SELECT output; BIND can replace
a context binding. Strict SEP-0007 substitutions have different scope and
conflict rules and keep their existing API.

Transport parameter terms through the existing typed exact-term boundary.
Never interpolate values into query text or invoke RDFLib's Python evaluator.
Execute against the caller's selected graph, union dataset or pending native
transaction. Invalid parameters abort an explicit transaction even when the
caller catches the error.

The explicit evaluator initial-context mode lowers original parsed algebra
with RDFLib's lazy-join eligibility and OPTIONAL retry variable scope. Preserve
that metadata before lowering basic graph patterns: a basic graph pattern's
internal joins differ from original nested Join, DISTINCT and Slice barriers.
EXISTS and other expression children use the same lowering mode. Ordinary
SPARQL preparation remains unchanged.

RDFLib-mode lazy joins stream the correlated right side, restore global initial
bindings when creating its context, and merge the left row with RDFLib's
precedence. OPTIONAL streams matches and retries with the original source
variable scope. MINUS keeps its independently evaluated sides and retains
global binding domains. Hidden bindings are removed only at the final public
projection, including DESCRIBE's selected resources.

Do not apply the ordinary SPARQL optimizer to this order-sensitive algebra.
Its join reordering and projection assumptions are invalid for these context
semantics. The native indexed dataset adapter remains the execution backend;
this does not copy the input relation. Query-plan caching for ordinary SPARQL
continues to use the existing bounded cache. Parameter values are never cached.

Verify native algebra regressions, paired original-RDFLib query results,
projection and barrier behavior, exact terms, selected contexts, transaction
read-your-writes and rollback, and the installed wheel's Python coverage.
Prepared Python query objects and initial SPARQL Update bindings remain
separate unsupported contracts.
