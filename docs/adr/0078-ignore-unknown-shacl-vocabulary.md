# ADR 0078: Treat unknown SHACL vocabulary as annotations

Status: Accepted; refines pyshacl-turbo ADR 0001 item 4 for shapes-graph vocabulary

## Context

pyshacl-turbo is a drop-in replacement for pySHACL. The native SHACL engine
(`crates/engine`) refused every `sh:` predicate on an executable resource and
every `sh:` type outside a hard-coded allow-list. This was intended to catch
misspelled terms, but pySHACL never inspects such terms: a shape's constraint
components are only the predicates in its parameter map (pySHACL 0.40.2,
`pyshacl/shape.py`, `Shape.validate`: `parameters = (p for p, v in
self.sg.predicate_objects(self.node) if p in search_parameters)`), and shapes,
targets, rules and functions are discovered from known types and predicates
only. A shapes graph using newer (for example SHACL 1.2) or misspelled
vocabulary therefore validated in pySHACL and failed in turbo.

## Decision

Unknown `sh:` predicates and types are annotations, as in pySHACL. They neither
fail compilation nor change the result.

Refuse explicitly only what pySHACL can execute and this engine cannot, so a
result never silently diverges from the reference:

- SHACL-JS: the predicates `sh:js`, `sh:jsFunctionName`, `sh:jsLibrary` and
  `sh:jsLibraryURL` on executable resources, and any `sh:JS*` class (for
  example `sh:JSConstraint`, `sh:JSFunction`, `sh:JSRule`, `sh:JSTargetType`).
  pySHACL executes these with `js=True` and ignores them otherwise; turbo has
  no JavaScript engine, so it fails in both modes rather than guessing which
  one the caller meant.
- In advanced mode, a bare `sh:SHACLFunction` that pySHACL registers globally.

All other executable vocabulary pySHACL implements (its constraint parameter
map, targets, rules, functions and expressions) is implemented natively, so
ignoring the remaining terms cannot drop a reference result. Structural errors
pySHACL also raises, such as a `sh:rule` that is neither a TripleRule nor a
SPARQLRule, remain errors.

## Consequences

A misspelled term is silently ignored, exactly as in pySHACL. Adding a new
executable feature to the engine must also add it to the native compiler; there
is no allow-list to update. Regression tests compare reports with the pinned
reference for shapes using unknown terms and assert the SHACL-JS refusals.
