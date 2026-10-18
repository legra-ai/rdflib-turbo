# ADR 0025: Transport graph metadata as typed terms

Status: Accepted

Native graph counts and graph-name membership use indexes, but the RDFLib facade
still serialized graph identities and wildcard patterns as JSON on every call.
That conversion is avoidable work for operations whose native memory lookup is
constant-time. It remains visible after removing their dataset-sized scans.

Use the existing typed term boundary for scoped graph length and named-graph
membership. Committed reads pin the caller's Python strings through detached
validation and native lookup. No Python reference enters native storage or a
returned snapshot. Transaction calls move owned input tuples to the existing
owner thread, which validates them before using the transaction. Invalid native
operations abort that owner even when Python catches the error.

Default graph identity is None for scoped counts; named membership requires an
IRI or blank node. Reuse the same term-position and metadata validators as other
typed operations. Do not cache counts or graph existence in Python. Keep closed
handle checks and committed-reader isolation intact. Replace the private JSON
membership argument directly rather than retaining a second compatibility path.

Graph registration serializes only when it actually creates a named graph.
Union-distinct counts and arbitrary selective count operations retain their
existing semantics; they cannot use a scoped whole-graph counter. Removing
transport overhead does not establish a universal performance improvement.

Regressions first reject JSON graph/pattern serialization during metadata reads,
then cover relative and blank graph identities, default and named counts,
transaction isolation, invalid metadata, abort, close and persistent empty graphs.
Installed-wheel measurements determine the resulting per-call cost.
