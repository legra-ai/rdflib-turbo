# ADR 0029: Select graph-scoped Dataset indexes before decoding

Status: Accepted

Dataset pattern queries used global indexes followed by a graph filter, even for
a known graph. A scoped subject lookup decoded 1,001 candidates to return one row.
The evaluator adapter also cloned graph lexical values into query closures; a
prebuilt 64 KiB graph identifier caused about 66 KiB of allocation per lookup.

Encode the four optional filters before returning an iterator. Resolve retained
filter references from the Dataset's own interner so iterator output borrows only
the Dataset, independently of caller filter lifetimes. Unknown terms return an
empty iterator. For a fixed graph, delegate to the existing GraphView pattern
dispatcher and its scoped indexes. Preserve global selection when no graph is
specified. No whole Dataset clone, caller-owned reference or second relation is
retained.

The evaluator adapter delegates validated filters to this shared Dataset method.
Keep SPARQL graph semantics explicit: a wildcard graph means named graphs only;
the default graph remains an explicit scope. Preserve invalid subject, predicate
and graph-kind behavior, exact terms, existing result order and RDF 1.2 terms.

Deterministic vendor regressions count actual decoded range candidates and
successful point lookups with test-only per-thread instrumentation. The one-row
fixture must visit one candidate, including with 1,000 unrelated named graphs.
Lifetime tests consume results after temporary filters are dropped. An isolated
allocation budget rejects copying a large graph binding and joins the required CI
inventory. Actual patched Dataset/evaluator feature-mode suites exercise these
contracts.

This first implementation decodes Dataset-owned filter references before the
existing GraphView dispatcher encodes subject/predicate/object filters again.
It removes graph scans and lexical copying but may hash long bound terms twice.
Installed-wheel CPU controls must measure this cost; do not claim universal query
improvement from row counts or allocation budgets alone.
