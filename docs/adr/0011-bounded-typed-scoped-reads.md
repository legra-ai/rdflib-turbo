# ADR 0011: Bound typed reads from an ordinary graph

Status: Accepted

The rdfpandas measurements show that native graph storage uses less memory at
10,000 records, but the read adapter reverses those savings. It collects quads,
serializes JSON, parses Python dictionaries, constructs all three RDF terms,
and groups all rows before yielding the first triple. Projections repeat term
construction even for fields the application never requests.

Ordinary Store reads of one graph use an owned native snapshot iterator. Return
typed term tuples in batches capped at 128 rows and approximately 64 KiB of term
payload. One indivisible RDF term may exceed that byte budget; at most one pending
row is retained. Release snapshots on exhaustion, explicit close or destruction.
There is no global term cache or mirror of RDF data in Python.

Graph projections request only the required fields. Bound RDFLib terms are
reused from the pattern. Exact lexical spelling, explicit datatype identity,
language tags, blank nodes and graph scopes retain the existing storage contract.
Property paths, unique projections, list selectors and subclass overrides retain
their RDFLib facade behavior while reading the same native storage.

This decision applies to committed, graph-scoped scans. Explicit transactions
retain their existing owner-thread protocol: their borrowed iterators cannot
escape safely. Union scans retain their existing grouping semantics. The default
in-memory backend does not return SPO-sorted rows, so adjacent-only deduplication
would be incorrect. Neither path silently switches to a committed snapshot or
copies into a reference graph. Bounded transaction and union iteration require
separate designs and regressions; this slice makes no claim about their memory.

Regression tests prohibit JSON/full-triple transport on ordinary scoped reads,
check projections and exact terms, and verify batch limits, early close, snapshot
ownership, nested reads and graph scopes. Measure installed wheels and complete
outputs before drawing performance conclusions.
