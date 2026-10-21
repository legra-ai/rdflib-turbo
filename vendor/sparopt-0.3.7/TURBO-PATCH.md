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
