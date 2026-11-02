# ADR 0074: Preserve absent DATA blank-node report identity

Status: Accepted

## Evidence

Updating the independent test reference to pySHACL 0.40.2 exposes an empty-DATA
report difference: a SHAPES `sh:targetNode` blank node absent from DATA prints its
identifier in the reference, while the engine prints `[ ]`. The reference now
selects DATA by presence rather than graph truthiness. Its dataset context lookup
raises LookupError for an absent node; the report consumer prints the identifier.
The deterministic Python regression retains equal conformance and isomorphic
report RDF and compares complete text with an explicit `absent-focus` identifier.

## Decision

Use the existing incident-context search in the native description renderer.
For a top-level DATA blank term with no subject or object occurrence, return its
identifier. Do not introduce another graph scan, a Python report rewrite, a
SHAPES substitution or a copied dataset. SHAPES descriptions and incident DATA
nodes retain their existing context selection, empty descriptions, nested
expansion, memoization and recursion limits. No RDF report statement changes.

The provider CI cohort uses the immutable pySHACL 0.40.2 facade candidate,
including its unchanged full-report regressions. Its independently retained
release-lock declaration remains a separate authority. Candidate results do not
establish public registry availability; the facade must require an independently
verified published provider before adopting this behavior in a release.
