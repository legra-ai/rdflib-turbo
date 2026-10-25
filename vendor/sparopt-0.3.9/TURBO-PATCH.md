# Local ordered query lowering

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


Raw EXISTS scope traversal differs from translated query scope: raw Bind includes
expression-only names, InlineData exposes its declared VALUES names, and a
SubSelect contributes its preserved source PV rather than hidden execution names.
The single scope walker carries an explicit raw-expression traversal mode.
OPTIONAL metadata is optional: None denotes a translated EXISTS graph that never
received RDFLib's `_addVars` pass and therefore skips the unmatched-row retry.
Pinned source-set and explicit-versus-wildcard result regressions protect these
observed distinctions; no alternate Python evaluator is used.
