# Local patch: externally bound aggregate projection variables

Upstream: Oxigraph spargebra 0.4.7, Apache-2.0/MIT; original license files retained.

The unmodified parser rejects `SELECT $this ?p (COUNT(?v) AS ?n) ... GROUP BY ?p`
because `$this` is not a grouping variable. SHACL supplies `$this` externally as
a constant for that evaluation. The opt-in `with_external_variable` API makes
those bindings visible to projection validation, without relaxing ordinary
SPARQL queries. It does not rewrite aggregates, groups, or query text.

Regression: tests/test_sparql.py::test_external_focus_variable_is_legal_in_aggregate_subquery.

## Opt-in RDFLib EXISTS query contexts

`SparqlParser::with_rdflib_query_context` preserves group-wrapper joins inside
EXISTS and NOT EXISTS. RDFLib 7.6.0 translates these expression graphs without
the query-level simplify/analyse pass: direct BIND, grouped BIND, and subqueries
therefore have distinct context/compatibility behavior. Ordinary groups and
default parsing retain their prior behavior. Scope is per parse; opening-brace
lookahead prevents a prefixed function from changing parser scope when PEG
alternatives backtrack. The focused rdflib_context_tests retain the distinction.


SubSelects inside this context retain `ProjectWithSourceScope`: execution variables
remain separate from raw source projection names (empty for SELECT *). RDFLib's
raw EXISTS `_addVars` treats explicit and wildcard projection differently even
when the resulting execution projections are identical. The default parser
continues to emit ordinary Project. Scoped pretty-printing retains wildcard
syntax; lowering consumes the annotation into an ordinary execution projection.
