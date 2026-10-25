# Local patches to Oxigraph 0.5.11

Original published archive SHA-256:
`7f6632dd080bd07676f06e70254ff5862c9943e9483d700fa7258df73a3f9a6a`.
Original source revision: `df37a5c98e2497135cdd4cfce01a049b78ca6740`.
MIT and Apache-2.0 license texts are retained from upstream v0.5.11.

The RocksDB feature activates its optional dependency with Cargo's explicit
`dep:` syntax. This keeps the implementation dependency private rather than
creating another implicit semantic feature. The separately attested backend
capabilities remain rocksdb, rocksdb-debug and rocksdb-pkg-config; activating
pkg-config still activates the same dependency's pkg-config feature.

The opt-in `exact-term-storage` feature stores explicitly typed literals with the
existing generic typed-literal encoding. Lexical spelling, declared datatype and
plain versus explicit xsd:string identity survive insert, query, update and
transaction commit. No additional indexes or side tables are introduced.

With the feature enabled, the dataset adapter uses QueryableDataset's RDF-term
internalization and decoded effective-boolean-value methods instead of the
upstream native numeric shortcuts. Value expressions still parse numeric values;
computed RDF terms use the same encoding as stored RDF terms. The feature also
enables spareval's local `exact-term-builtins` feature for STR and DATATYPE.
Without the opt-in, upstream encoding and shortcuts remain unchanged.

Exact-term lexical dictionary keys use the existing siphasher dependency's
128-bit SipHash-1-3 implementation. The key width and byte order are retained.
Persistent exact stores atomically declare `turbo-strhash = siphash13-128`
together with their initial storage metadata. Missing or mismatched declarations
on existing stores are hard errors on read-write and read-only open. Exact stores
do not execute upstream storage migrations. Feature-off hashing and storage
behavior remain upstream. See ADR 0033 and native storage contract tests.

See ADR 0006 and `tests/store-contract` for the feature matrix and persistence
regressions. This dependency patch does not itself expose a Python Store API.

`store::QuadIter::project` consumes an existing encoded cursor and returns
`ProjectedQuadIter`, decoding only selected subject/predicate/object terms. Graph
names are not decoded by this projection. Cursor ownership, snapshot lifetime and
transaction borrows are preserved without another storage reader or result copy.
The implementation is in `src/store/projection.rs`, exported by `src/store.rs`;
`tests/store-contract/src/projection_tests.rs` covers all masks and source lifetimes.
See ADR 0013 for the graph-scoped Python integration boundary.

`store::QuadIter::project_lexical` consumes the same encoded cursor and returns
`LexicalQuadIter`. Selected `LexicalTerm` values retain their RDF kind and exact
literal metadata, including directional languages and recursive triple terms
when RDF 1.2 is enabled. `TermText::as_str` borrows an immutable text owner:
inline small strings stay inline, memory dictionary values lease an `Arc<str>`,
and persistent values and native numeric formatter output retain owned strings.
No workspace-sized result collection, Python object cache, new dictionary or
additional snapshot is introduced. The old `.project()` and copy-producing
`StrLookup::get_str` APIs retain their behavior.

The memory dictionary now owns immutable `Arc<str>` values. A lease clones its
owner while guarded, then releases the DashMap guard before returning. This
removes transient lexical payload copies in the projection boundary without
letting a dictionary lock or mutable view escape. The ownership header changes
per-entry memory overhead; actual RSS and CPU effects require installed-wheel
measurements and are not inferred from pointer sharing alone.

Implementation: `src/storage/term_text.rs`, `src/store/lexical.rs`, and the text
lookup methods in `src/storage/{memory,mod}.rs`. The existing published-archive
hash above continues to identify the unmodified upstream archive; these are local
source patches. Regression coverage in
`tests/store-contract/src/lexical_projection_tests.rs` checks shared pointers for
long identifiers and a 4 MiB literal, complete projection masks, old/new decoded
term equality, exact datatypes and language tags, numeric encodings, retained
snapshots, transaction borrows, RDF 1.2 direction/triples and persistent reopen.

Memory storage publishes exact total and per-graph cardinality with its committed
version. Readers capture total counts at snapshot creation; transactions update
working counts only on visibility transitions. A single touched graph stays
inline, while additional graph counters use native index metadata. The new
`Store::graph_len` and `Transaction::graph_len` operations use these counters for
memory and authoritative encoded-index scans for persistent storage. No union
deduplication or physical reclamation semantics change. Implementation lives in
`src/storage/memory_counts.rs` and the storage/store adapters; regressions are in
`src/storage/memory_count_tests.rs` and the standalone cardinality contracts.
See ADR 0023 for publication and ownership invariants.

`Store::read_snapshot` exposes a pinned committed MVCC reader without acquiring
the writer lock. Indexed quad and named-graph reads share that version for a
complete SHACL validation. The view owns storage reader metadata, not a quad
copy; pending writer changes remain invisible. This prevents reentrant writer
deadlocks during non-inplace validation. See ADR 0037 and shacl_read tests.

Expose the evaluator's explicit owned RDFLib prepared algebra on
PreparedSparqlQuery. Query form, dataset, base and template remain typed query
fields; there is no query-text reconstruction path.

## Typed default graph for scoped RDFLib UPDATE

`PreparedSparqlUpdate.with_default_graph(GraphName)` selects implicit WHERE reads
and default write targets on the same native transaction, including blank-node
graph identifiers that cannot be represented in SPARQL source syntax. Explicit
WITH/USING datasets and GRAPH output templates keep their own selections. Scoped
updates use readable transactions so pending writes remain visible; no source
dataset is copied. Default INSERT/DELETE DATA, DELETE/INSERT WHERE and CLEAR/DROP
operate on the supplied graph. Replacing the builder scope updates reads and
writes together; original explicit dataset specifications remain immutable. This source patch must be byte-identical in the
canonical companion engine dependency under ADR0048.

Committed memory pattern selection stops probing later SPOG indexes once a
supplied component has no index entry. The absent component keeps its zero
cardinality and therefore remains the selected empty chain. Existing historical
chains still undergo snapshot filtering; zero visible cardinality is not used as
proof of absence. No cache, index, retained pattern or production counter is
added. Per-reader test instrumentation counts actual index probes; regressions
cover all missing component positions, all sixteen masks, commits, rollback and
retained snapshots. Installed-wheel measurements report boundary costs separately.

## Exact persistent prefix bounds

RocksDB prefix scans increment the last non-`0xff` byte and truncate the carried
suffix when constructing their exclusive upper bound. Retaining trailing
`0xff` bytes admitted adjacent graph or term identities, including maximum
numeric blank-node IDs. Empty and all-`0xff` prefixes have no finite successor;
their unbounded upper limit is the mathematical range, not an alternate lookup.
Encoded term tags, index key bytes and the on-disk version remain unchanged.

Raw-byte regressions cover carry and both unbounded cases. Public Store tests
compare memory and persistent results across all sixteen masks, default/named/
blank scopes, maximum 64/128-bit IDs, empty blank labels, pending writes,
rollback, pinned snapshots and reopen. Both native storage profiles require
these regressions in CI. This preserves the exact-term and scoped transaction
contracts in [ADR 0006](../../docs/adr/0006-exact-term-transactional-store.md)
and [ADR 0007](../../docs/adr/0007-store-api-and-scoped-transactions.md).

The encoded quad graph slot uses the graph-only domain from ADR 0060 rather than
retaining a full literal-capable term. All default, named and full-width blank
representations remain unchanged. Checked decoding rejects invalid literal graph
slots; converting valid values through the existing term codec preserves exact
v1 bytes. Graph dictionaries and indexes retain their existing types. Native
layout, nine index-order golden bytes, scope/transaction/snapshot and reopen
regressions are mandatory in both storage profiles. Allocator/RSS savings remain
unmeasured by these source tests.

## Borrowed generalized point membership

Store and Transaction expose contains_generalized on borrowed full-Term quads.
It encodes the requested terms directly into the existing quad key and invokes
the same versioned reader.contains path as ordinary membership. Stored terms
are never decoded for this boolean, and no storage index, wire shape or
publication behavior changes. Native controls cover all subject/predicate kinds,
exact default/named/blank contexts and pending mutation rollback; the existing
isolated transaction membership budget also checks long generalized roles on
both Store and Transaction without replacing its ordinary membership guard.

The bound prepared-query wrapper exposes the separate
`execute_inference_construct` capability from the patched evaluator. It retains
substitutions, dataset specification and the actual bound source, returning
full-Term heads without changing ordinary execute/QueryResults::Graph. The
inference-only evaluator rejects other query forms and propagates source errors;
publication and predicate policy remain at the provider's Pending boundary.

GeneralizedQuadIter has an explicit lexical projection capability retaining its
existing owning Arc and encoded iterator. Strict QuadIter projection retains its
positional validation. Both project selected fields through one compile-time
term-domain helper; full-Term projection accepts every stored subject/predicate
kind without copying its dictionary or constructing temporary model strings.
Mandatory owner regressions cover all roles/masks, strict errors and lexical
lifetimes after source mutation and drop. No encoded wire/index shape changes.
