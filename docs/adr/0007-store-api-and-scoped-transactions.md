# ADR 0007: Expose a Store with scoped native transactions

Status: Accepted

ADR 0006 permits Store adoption after exact RDF identity has been established.
Expose Store as the storage API now. Dataset keeps its current inference/SHACL
implementation until both engines have direct Store/transaction adapters. Do not
route a Store through an in-memory Dataset to make those APIs appear supported.

Store() creates an in-memory Oxigraph store. Store(path) opens RocksDB. The Rust
persistence feature is optional for Rust builds; published Python wheels enable
it, while the Python constructor still defaults to memory. Unsupported builds
reject paths explicitly. close() releases the handle and rejects an active
transaction. No namespace persistence or RDFLib Store plugin contract is claimed.

Parsing and SPARQL Update use implicit transactions. An explicit Python context
exposes parse, update, query, serialize and length on its uncommitted view. Normal
exit commits; exceptions abort. A native operation failure aborts the entire
transaction, even if Python catches that failure. Reusing the closed transaction
fails. Readers on the Store see committed state, while its transaction sees its
own changes. Mutations and nested transactions on the same Store handle fail
immediately while a writer is active; they never wait for a caller-held writer.

Oxigraph's transaction borrows storage and may retain thread-affine lock guards.
Keep it on a dedicated Rust owner thread for its complete lifetime. The Python
handle owns rendezvous channels and a join handle, not a borrowed pointer, unsafe
lifetime extension or independently loaded extension object. Each call releases
the GIL while waiting. There is no background command queue. Terminal errors,
commit, abort and handle destruction disconnect/join the owner, ensuring rollback
and writer reservation release complete before returning. No Rust callback from
that thread invokes Python. Future inference/SHACL adapters execute there against
the same transaction.

The transaction retains changed data in Oxigraph's native delta. Never clone the
base dataset for rollback. RDF parsing feeds quads incrementally to the transaction;
parse errors abort previously inserted quads. Python documents and query/serialized
results remain eager API boundaries. This does not promise bounded memory for an
unbounded update delta, inference closure or query result.

Dataset, Store and transaction use shared Python input/result conversion and the
same typed Rust query-result serializer. Graph query output is serialized directly
without building a second indexed Dataset. Existing Dataset graph-scope semantics
and namespace metadata are unchanged. SHACL report commit/nonconformance policy
remains an explicit decision for the engine integration slice.
