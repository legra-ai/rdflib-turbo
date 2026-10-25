# ADR 0065: Generalized terms in the native data relation

Status: Accepted for the bounded model/storage seam; public wiring pending.

Original RDFLib programmatic graphs preserve URI, blank and literal terms in
subject and predicate positions. Original RDFS inference also retains statements
with literal subjects. Restricting the native relation to named/blank subjects
and IRI predicates therefore loses observable target state even when validation
reports agree. Removing one inference filter cannot repair storage, joins or
derived output which cannot represent those terms.

Introduce first-class generalized triple/quad data models with Term in all three
statement positions. Graph names retain their named/blank/default domain:
RDFLib normalizes Literal graph-identifier inputs to URIRef lexical identifiers.
Keep strict RDF syntax models separate. Promotion to the generalized model is
lossless; narrowing must explicitly report the first invalid subject/predicate
position, without cloning a literal payload into an error or silently omitting
a row.

Use the same existing native EncodedQuad relation, lexical dictionary, MVCC
versions, transaction journal and indexes. EncodedTerm and current index byte
keys already support literal term encodings in each slot; verify exact existing
v1 bytes instead of introducing a new format/version or migration. Generalized
reads decode Term subjects/predicates. Strict reads must fail explicitly when a
generalized row cannot be represented, never filter it or truncate iteration.
Owned read views retain the real pinned storage owner, not copied statements.

Derived/edit/tombstone indexes, GraphRead/QueryGraph, CONSTRUCT/inference head
publication, report terms and public Graph APIs must use the same generalized
data model before public parity is claimed. Never add another relation, fake
IRI/blank-node encoding, Python sidecar, reference evaluator or base copy.
Preserve domain-specific SHACL shape/path constructor checks independently of
the broader data-term domain.

Parser and serializer boundaries follow each original format's actual behavior.
In the pinned original, Turtle accepts generalized subjects/predicates whereas
N-Triples parsing rejects them. N-Triples/Turtle serializers emit such statements;
RDF/XML and JSON-LD omit the probed literal-subject statement. Do not infer all
format contracts from standards-only model types or silently normalize data.

Regression-first checks cover model identity and positional narrowing, all16
quad pattern masks, exact literal dictionary identity, memory/persistent
rollback/reopen, owned snapshot lifetime and existing byte encodings. Shared
vendor source bytes and effective features must match canonical/provider
fingerprints. Native execution coverage remains unmeasured until instrumented;
model tests alone establish neither public Graph parity nor a speed improvement.
No partially connected seam is published.

## Shared in-memory owner and staged inference

The in-memory generalized owner uses the same dictionary, immutable records and
six indexes as ordinary Dataset. Moving parsed Dataset ownership into
GeneralizedDataset moves these existing allocations. A common record holds a
full-Term subject and named predicate in 112 bytes; a non-IRI predicate uses an
explicit boxed variant. Ordinary Dataset/parser APIs retain their strict domain.

The mutation journal initially retains an 8-byte Arc per addition. Its first
actual removal promotes only changed keys into two disjoint first-presence sets.
Rollback removes initially absent keys, restores initially present original Arc
incarnations, then releases newly interned words. Delta iteration exposes current
records for initially absent keys; additive-only iteration avoids index lookups.
No base clone, second index implementation, dictionary GC or Store migration is
introduced. Requested layout sizes do not establish allocator RSS savings.

Internal inference selectors, rows, pending heads and procedural kernels retain
full-Term roles. Suppression checks the source and destination; destination-only
publication checks retain their distinct meaning. New literal-predicate heads
are refused explicitly while existing literal-predicate input remains present.
Blank predicates can be intermediate heads before reference postprocessing.
Public SPARQL CONSTRUCT remains strict. An explicit inference-only full-Term head
capability retains the existing template and compact duplicate-cache machinery,
rejects non-CONSTRUCT forms, and propagates actual source/externalization errors.
The inference catalogue invokes that explicit capability through each existing
backend with its original source scope. Public generalized ingress remains pending. No complete public OWL parity claim
follows from these internal reader, head and ownership gates.

Generalized in-memory query and SHACL adapters retain borrowed terms. Native
strict RDF export rejects an unrepresentable role explicitly without mutating or
filtering the owner. Raw language spelling, context-specific representatives and
MVCC language presentation remain unresolved rather than encoded in sidecars.

OWL postprocessing removes source-enumerated blank-predicate statements only
once all reasoning rounds and diagnostics succeed. A scoped source removes its
exact physical rows and matching SPOs in its destination; destination-only SPOs
are retained. Union cleanup preserves named/blank graph identity. In-memory
transactions resume through the existing ordered POSG index with one retained
Arc and journal only actual removals. Store transactions select only matching
rows, drop their borrowed query iterator, then use existing generalized point
removal. Neither path copies the base relation.

Borrowed inference keeps its source immutable and represents cleanup as exact
full-Term logical tombstones. Its semantic delta contains additions plus an
optional boxed removal dataset, allocated only when a source row is actually
removed. Read errors remain observable. Acceptance applies removals, inference
additions, then rule additions under the existing revision and warning policy.
Discarding readonly or failed staging never changes its caller. The public
mutating `infer_delta` contract still returns additions only; its transaction
journal excludes additions that postprocessing subsequently removes.

RDFS one-time equal-string substitution uses the same explicit full-Term query
projection. Its publication policy follows the original direct destination.add:
all term roles, including newly substituted literal predicates, are retained.
This exception is local to that RDFS one-time rule; OWL derived-head refusal is
unchanged. Query results remain immutable until distinct destination-absent
output has been staged. Source errors prevent publication. Eighteen exact
original role/scope graph-set fixtures retain both same-source and independent
destination behavior; ordinary public CONSTRUCT still uses strict projection.

Programmatic Store boundaries now validate complete RDF terms independently of
statement positions. Subjects and predicates retain the same full-Term domain
as storage. Graph names keep their named/blank/default domain, and malformed
literal metadata remains an error. Typed insertion, exact boolean membership,
JSON transaction selectors and projected reads use existing generalized storage
capabilities. A present generalized row must never be classified as an impossible
match. Fixed-graph counts retain encoded selection without lexical decoding.
Ordinary syntax parsers, serializers and strict projection capabilities keep
their separate contracts.

## Unpublished native record import

`NativeStore.from_records(records, declared_graphs, input_terms)` constructs a
fresh in-memory owner from full-Term four-tuples and explicit URI/blank graph
declarations. A `None` graph denotes the existing default graph. Keep the input
descriptor's caller-owned classes alive while staging and pinning each fixed
window of at most 128 records, then send the borrowed lexical views through the
existing transaction worker. Insert through the existing generalized point API;
do not serialize, copy a base relation, or build another indexed dataset.

The owner remains unpublished until both input streams succeed and the single
transaction commits. Iterator and metadata errors propagate unchanged; native
validation errors abort the writer. Empty named and blank domains use the
Store's existing graph metadata. This factory does not import Python namespace
state or implement inference delta capture.

## Private changed-only memory capture

An explicit new-memory-transaction capability records the initial presence of
actual changed quad identities. With capture disabled, the transaction retains
only a null optional pointer and allocates nothing for capture. Additions retain
one existing Arc per row; the first actual removal promotes them to disjoint
initially-absent and initially-present sets. Final presence cancels transient
add/remove or remove/reinsert sequences without comparing a copy of the base.
The existing mutation journal remains responsible for rollback.

Commit classifies the changed identities, publishes the transaction and creates
its owning committed reader while the original writer permit is still held.
Changed-only cursors share this reader/dictionary lease and retain only row
identities, including removed rows. No second indexed relation or lexical copy
is retained. Reject persistent transactions, repeated capture and prior pending
writes rather than inferring an unsupported initial state. This prerequisite
does not itself expose Python changes, alter public inference, or repair raw
language-tag presentation and incarnation semantics.

The private Python bridge starts its own memory transaction, captures native
diagnostics before allowing commit, and finishes through a distinct typed
`FinishChanges` command. Ordinary transaction finish and public inference keep
their existing routes. One frozen changes handle owns both full-quad lexical
cursors and the caller's restoration factories. Native batches release their
lock before invoking Python, retain at most 128 rows with a 64-KiB byte target,
and permit one oversized atomic row. Closing or a restoration failure releases
both continuations. Ordinary RDFLib writeback must close this handle on warning
or publication failure; it must not retain another indexed additions dataset.

Public additions materialization uses a distinct prepared capture. Finalize the
actual changed row identities without publishing, keep them and their original
transaction on the same Session worker, and decode bounded lexical batches
through that transaction's borrowed reader. No pending reader or borrowed cursor
escapes the worker. The prepared marker refuses ordinary commit, repeated
preparation, and unrelated worker operations. A typed acceptance consumes both
exhausted streams and publishes exactly once. Drop, explicit close, decoding
errors and rejected Python metadata abort the writer; pending worker lexical
buffers and changed identities are discarded before dictionary rollback.

The Python prepared handle also rejects acceptance during term restoration,
including the last native batch after its row iterator is already exhausted.
Factories run after native locks are released and may close the handle; their
original exceptions remain observable. RDFLib metadata is staged separately and
its revision checked before acceptance. The committed changed-only bridge stays
separate for unpublished private owners; it is not safe for public caller
publication followed by fallible Python output materialization.

An additions-only consumer may explicitly discard the prepared removal stream.
Consume its retained changed identities directly, without decoding lexical
fields, constructing Python terms or emitting conversion warnings. This marks
only removals complete; additions still require successful restoration before
acceptance. Discard is refused while a Python batch is in flight or after the
prepared handle is closed or accepted. The committed OWL writeback path still
reads and publishes its actual removal rows.

Dataset compatibility reads distinguish any, default and explicit named/blank
physical scopes. Union selection chooses one exact encoded-SPO representative
using the same pinned reader, before decoding statement fields and without a
visited set. Each returned union statement carries a separate lazy context
cursor built from that encoded SPO and the same reader Arc. It opens its indexed
context lookup and restores only graph identifiers when consumed. It must not
use a new Store snapshot or re-encode Python statement terms. Context cursors
remain valid after the outer iterator or Store closes, including rows from later
batches and contexts deleted by a later writer. Batch size and byte limits also
bound the number of retained row/context cursor pairs.
