# ADR 0017: Materialize RDFLib terms after native scanning

Status: Accepted

The remaining full-iteration profile contains one Python decoder dispatch and
one four-field Python descriptor tuple for every returned term. Native batches
already own the lexical fields, so this transport repeats allocation and dispatch
before producing the RDFLib objects required by the public API.

Keep pure native scanning, projection and bounded raw rows separate from Python
conversion. Detach from Python only while advancing the owned snapshot cursor.
After the cursor lock is released and the GIL is reacquired, construct actual
RDFLib identifier and Literal objects using caller-provided immutable factory
handles. Identifier restoration uses the accepted string allocator from ADR 0015;
Literal restoration uses the shared state factory from ADR 0016. PyO3 calls use
Rust argument tuples and its positional-call path. No module import, Python
callback or Python factory runs inside a Rust cursor lock or detached region.

The first bounded batch returns with an optional continuation. Only unfinished
scans allocate a Python cursor, which owns the pure cursor and its factory
handles. Later batches retain the 128-row and approximate 64 KiB payload bounds,
including the existing one-pending-row and indivisible-large-term rules.
Projection placeholders remain None and callers reuse already-bound terms.

A Python conversion error drops an initial owned continuation or closes an
existing continuation, preserving the original Python exception. Explicit close,
exhaustion and destruction release snapshots. Python GC traversal visits all
factory handles so a callback capturing its continuation does not create an
uncollectable reference cycle. There is no mutable global cache or Python RDF
mirror. Transaction-owner and union protocols retain their separate contracts.

Regressions check actual RDFLib classes and public Literal state, projection
placeholders, batch bounds, early close, factory errors, callback reentry,
reference-cycle collection and invalid factory arguments. Pure native tests
retain exact lexical identity and independent lexical ownership assertions (ADR 0018). Installed
wheels and complete workloads determine performance results.
