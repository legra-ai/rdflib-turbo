# ADR 0023: Publish native memory cardinality with snapshot versions

Status: Accepted

Counting encoded matches avoided lexical decoding but still visited every retained
quad. Memory storage also retains historical nodes for MVCC, so repeated `len`
could cost more than the currently visible dataset size.

Publish the committed version, total quad count and exact graph counts under one
storage-owned lock. Each memory reader captures its version and total together;
its length and emptiness queries use those captured scalars. Old readers retain
their original cardinality without a version ledger or graph-map copy.

The single writer maintains transaction-local working counts. Only actual quad
visibility transitions change counts; duplicate additions and absent removals do
not. Clear, graph removal, parsing and bulk loading use the same mutation hooks.
Abort discards pending counts. Commit publishes counts only after upgrading the
MVCC ranges, with no index/range guards retained during publication. Overflow and
underflow fail immediately. Named graph existence remains separate from count.

Capture default-graph cardinality alongside the version and total at transaction
start. Pending default changes and reads use that owned scalar, avoiding a second
publication-lock acquisition on the first mutation. Commit publishes the scalar
directly; the committed graph map contains only nondefault graphs, so default
publication and graph-length reads do not hash a synthetic default key. Lazy
baseline callbacks are used only when an untouched named graph needs its initial
count; regressions panic if default reads or mutations consult that callback.

The first touched named graph keeps its graph count inline. Additional touched
named graphs use native scalar metadata. Committed graph metadata scales with
nonempty graphs, never copies statements, and is not cloned for snapshots.

`Store.graph_len` reads latest committed cardinality; `Transaction.graph_len`
includes pending changes. No arbitrary old-reader graph-count API is introduced.
The persistent backend implements the same exact operation by scanning its encoded
graph index. Only memory counts are constant-time; approximate RocksDB estimates
and missing-metadata recovery paths are not used. Union-distinct triple counts
and selective pattern counts remain separate queries.

Tests compare counts with independently iterated visible statements across
mutation, abort, clear, graph scopes, bulk loading and persistent reopen. A native
reader-local test counter proves length and emptiness visit no physical history;
concurrent snapshots verify version/count publication. This decision does not
reclaim historical nodes or dictionary payloads and claims no RSS improvement.

`make test-memory-storage`, included in `test-store-contract` CI, runs the actual
vendored library's memory unit tests, including deterministic scan instrumentation.
The runner forwards the root manifest's dependency patches into Cargo and uses
the vendored lockfile. Upstream development dependencies are retained; no copied
library source or separately maintained test dependency manifest is used.
