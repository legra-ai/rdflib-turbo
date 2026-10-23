# ADR 0005: Journal inference insertions instead of cloning the dataset

Status: Accepted

ADR 0001 requires atomic parsing and inference. Inference previously achieved
this by cloning the complete indexed dataset, expanding the clone and replacing
the original on success. It also retained every CONSTRUCT result in a round,
including existing facts and duplicates across rules. Phase allocation profiles
show inference, rather than validation, dominating the transient heap peak.

Preserve the atomic contract with an additive transaction in the patched oxrdf
dataset. Its guard exposes immutable dataset reads and journaled insertions only;
there is no mutable dataset dereference. The journal stores compact interned quad
keys, not owned RDF strings or copies of existing indexes. Commit drops the
journal. Error or unwind removes inserted quads from all six indexes before
releasing newly interned strings, generated blank-node IDs and RDF 1.2 triple
entries. Interner capacities grown by a failed transaction are shrunk toward
their original capacities. Ordinary dataset clones remain independent snapshots,
including when cloned from a transaction's immutable view.

Hold the existing native write lock for the entire operation. No reader can see
intermediate inference rounds. A panic still poisons the lock and fails later
operations explicitly. The journal is not a change to inference semantics or a
promise to recover from process abort/OOM.

Retain only distinct facts absent from the current round's dataset in the pending
set. Insert them after all rules finish reading that round. This preserves round
boundaries and fixed-point semantics. Stream the initial RDFS resource candidates
instead of constructing a duplicate-heavy Vec first. Query evaluation still has
its own allocations; this change does not eliminate all intermediate results.

Named-graph/destination inference still materializes one union working dataset,
as required by graph-scope semantics. Journal only additions to that view, then
commit those inferred quads to the requested destination after complete success.
This removes the separate before/after union clones and does not copy projected
original named-graph triples into the destination. Default-graph inference does
not copy the input dataset at all.

Vendor transaction tests cover commit, duplicate/preexisting facts, exact literal
identity, all index lookups, generated blank nodes, RDF 1.2 quoted triples,
independent snapshots, and unwind rollback. Runtime regressions trigger failure
after an inference round has already inserted derived facts, for default/named
inputs and default/named destinations, then repair and reuse the input.

The optional allocation-profiling feature instruments a release test binary only.
Run its ignored tests alone with one test thread. Counts are requested live/peak
Rust heap bytes, not Python memory, allocator arena overhead or process RSS. CI
enforces a deterministic inference peak budget relative to retained heap; the
original implementation fails that budget. Installed-wheel pipeline benchmarks
remain the source for timings, whole-process peak RSS and full output parity.

## Boolean inference membership

Boolean probes borrow bound terms through `QuadRef` instead of cloning a quad or
materializing a matching row. Default Dataset membership still checks only the
default graph. Scoped transaction suppression still checks source A or existing
destination B, while direct destination publication checks only B. Union reads
retain their existing any-context behavior; an exact default-graph membership
check cannot replace a union probe.

A borrowed inference overlay probes its scoped source first and propagates read
errors before consulting its derived-only Dataset. Every overlay fact belongs
to the configured destination, so its existence can use a borrowed exact lookup
without reconstructing a logical-union representative. This does not remove the
owned source row required by the current GraphRead interface.

Scope, term-kind, exact literal, pending transaction and rollback controls remain
required. Isolated allocation tests use existing test-only counters and long
lexical values; they measure requested bytes for one boolean probe, not process
RSS or end-to-end speed. No storage, dictionary, public API or publication change
is introduced.
