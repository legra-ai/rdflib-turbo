# Local patch: externally bound aggregate projection variables

Upstream: Oxigraph spargebra 0.4.7, Apache-2.0/MIT; original license files retained.

The unmodified parser rejects `SELECT $this ?p (COUNT(?v) AS ?n) ... GROUP BY ?p`
because `$this` is not a grouping variable. SHACL supplies `$this` externally as
a constant for that evaluation. The opt-in `with_external_variable` API makes
those bindings visible to projection validation, without relaxing ordinary
SPARQL queries. It does not rewrite aggregates, groups, or query text.

Regression: tests/test_sparql.py::test_external_focus_variable_is_legal_in_aggregate_subquery.
