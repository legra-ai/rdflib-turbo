# Local changes to sparopt 0.3.7

The vendored published source retains its MIT and Apache-2.0 licenses.

Positive `FILTER(BOUND(?variable))` conditions, including positive conjunctions,
remove the undefined alternative from inferred variable types. Retain every
existing RDF-kind restriction; filtering cannot introduce another term kind.
Do not infer the same fact through OR or NOT. This allows computed value keys
to select an actual keyed hash join instead of a Cartesian product.

Regression tests cover nullable VALUES bindings, conjunction and negative
controls. The native RDFS string-substitution regression also inspects the real
prepared query plan, in addition to checking complete reference output.

## Ordered query lowering

The upstream crate is sparopt 0.3.7 from the Oxigraph project (MIT OR Apache-2.0).

`GraphPattern::from_rdflib_algebra` retains original query evaluation boundaries.
Lazy-join eligibility is determined from source algebra before BGP lowering adds
internal joins. Original Join, DISTINCT, and SLICE nodes block parent lazy joins.
OPTIONAL carries the original left-variable scope used by RDFLib 7.6.0's retry,
including its VALUES-row, hidden-projection, and BIND-expression distinctions.
Translated EXISTS graphs retain context handling without query-level lazy analysis.
The enclosing query's lazy analysis sees raw EXISTS syntax, not the translated
expression graph; internal group/Distinct/Slice nodes therefore do not block an
outer lazy join. Tests retain both sides of this source/translation distinction.

Only individual BGP leaves use the existing optimizer, allowing indexed/shared-key
plans without moving BIND/projection/group boundaries. The explicit unoptimized
entry point disables even leaf optimization. Ordinary From conversion and the
standard optimizer entry points remain unchanged. Tests cover source-node identity,
EXISTS wrappers, OPTIONAL scope, and optimized/unoptimized leaf plans.
