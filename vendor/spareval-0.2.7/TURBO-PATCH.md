# Local patches to spareval 0.2.7

Upstream Oxigraph revision: `0e81a29d27575ab69e7f85ea9c7f1058b277fcd0`.
MIT and Apache-2.0 license texts and original archive provenance are retained.

The prebinding patch supplies initially bound variables to the optimizer;
see `../sparql-prebinding.patch` and ADR 0004.

The explicit `with_initial_bindings` API evaluates RDFLib query contexts, separate
from strict SEP-0007 substitutions. It retains hidden initial variables while
projecting only declared SELECT/DESCRIBE outputs. Its ordered sparopt lowering
preserves lazy-join and OPTIONAL scope metadata; native streaming evaluation
matches correlated-context merge precedence and global-binding reinstatement.
Ordinary SPARQL preparation and strict substitution scope remain unchanged.
See rdflib-turbo ADR 0042 and native/public initial-binding regressions.

The opt-in `cached-query-plans` feature reuses immutable optimized graph patterns
in a process-local 128-entry LRU. Exact parsed patterns and sorted initial bound
variable names form the key; execution values, current data and registered
callbacks are never cached. SELECT, ASK, CONSTRUCT and DESCRIBE share this path.
Explicit `without_optimizations()` bypasses the cache. Without the feature, each
execution still performs its own optimization. See ADR 0025 and `src/plan_cache.rs`.

The opt-in `undefined-functions-as-expression-errors` feature changes only the
unregistered scalar-function arm in expression evaluator construction. An
unknown function returns an expression error (`None`) at evaluation time, so
FILTER, BIND, COALESCE and logical operators can handle it. Without this feature,
upstream's UnsupportedCustomFunction construction error remains unchanged.
Registered callbacks and unsupported aggregates are not changed by this patch.

The engine enables this feature for pySHACL compatibility under ADR 0012.
`tests/test_undefined_functions.py` checks the expression contexts, and
`test_sparql_function_error_cannot_become_a_conforming_report` keeps registered
callback failures visible.

The separate opt-in `exact-custom-function-terms` feature retains RDF terms
across registered custom-function calls. Arguments and returned terms bypass
`ExpressionTerm` canonicalization, preserving noncanonical numeric lexical
forms and plain versus explicitly typed strings. Enclosing value expressions
continue to request value semantics. The feature enables oxrdf's local
`rdf-10-string-identity` feature and is used under ADR 0015.

This feature also preserves terms produced by `xsd:string(...)` and `STRDT`,
including `IF` and `COALESCE` branches containing ordinary value expressions.
Explicit string constructors must not pass through `ExpressionTerm::StringLiteral`,
which loses their datatype spelling. `STR` and `CONCAT` continue to produce plain
strings. See ADR 0022 and `tests/test_literal_identity.py`.

`make test-sparql-evaluator` tests upstream callback behavior with the feature
disabled and exact terms with Turbo's features enabled. The standalone manifest
uses the same local oxrdf/spargebra/sparopt patches, with its own pinned lockfile.
It is excluded from the main workspace to prevent feature unification from
silently enabling the compatibility features during the default-mode test.
# Borrowed scoped custom functions

`QueryableDataset::scoped_custom_function` supplies a lifetime-bound Rc callback
for functions that query the same borrowed source. Explicit evaluator-registered
functions take precedence. The expression evaluator resolves both ordinary and
exact-term calls through this capability. No callback or source is retained
beyond the query lifetime, and the source need not be cloned into an Arc Dataset.


The opt-in `exact-term-builtins` feature enables `exact-custom-function-terms`
and evaluates STR and DATATYPE against original RDF terms when available. Numeric
value expressions retain normal value semantics. Without the feature, the upstream
builtin paths remain unchanged. The transactional Store enables this feature;
see rdflib-turbo ADR 0006 and `src/store_tests.rs`.

The opt-in `rdf-set-union` feature deduplicates exact subject/predicate/object
tuples when multiple source graphs form the query's default graph. Deduplication
happens before graph-pattern evaluation, preserving SPARQL solution bags; it is
not SELECT DISTINCT or output filtering. Both union-of-all-graphs and explicit
multi-FROM datasets use this path. Feature-off upstream behavior is unchanged.
Native Graph/StoreDataset regressions retain two identical projected bindings
from two distinct triples while collapsing copies of the same triple across
contexts. See rdflib-turbo ADR 0008.

CONSTRUCT result deduplication now stores query-local exact term identities and
compact three-integer keys instead of retaining an owned copy of every triple.
Variable terms use the QueryableDataset sameTerm equality contract; template
constants keep bounded external identities, including variable/constant equality.
Constants and generated blanks do not require dataset internalization. Existing
validation, blank/quoted bypass and cache clearing remain intact. See rdflib-turbo
ADR 0028, persistent construct regressions and the unchanged inference heap budget.

The Dataset adapter delegates validated statement filters to the lifetime-safe
Dataset pattern iterator, eliminating copied graph bindings and unrelated graph
decoding. Preserve the SPARQL named-only wildcard graph rule separately. Exact
scope/kind/lifetime tests and the large-binding allocation budget cover ADR 0029.

# Independent RDFLib join lookup

Original independent joins distinguish the explicit solution domain from
initial-binding lookup. Directional compatibility consults initial values for
missing right-side keys without restoring hidden projected keys to the domain.
These joins stream left rows against a distinct right-hand solution set.
Ordinary SPARQL joins retain their existing compatibility rule.

# RDFLib update context

DELETE/INSERT preparation uses the same explicit RDFLib algebra lowering as
queries. Its private output projection retains initial globals absent from WHERE
so mutation templates can read them, while BIND can replace their local values.
Each prepared operation starts from the immutable original initial context.
Template variable lookup consults that context for keys absent from a projected
solution, without changing the projection's explicit row domain.
Ordinary SPARQL update evaluation retains its original output domain.


Prepared RDFLib query objects can supply an owned native plan whose exact source
algebra is checked before execution. Explicit join/OPTIONAL metadata remains in
that immutable plan; the evaluator neither reanalyses it nor shares it through
an object-identity cache. Source-query parsing and ordinary optimizer paths are
unchanged. Native prepared-query regressions cover the four forms and metadata.
