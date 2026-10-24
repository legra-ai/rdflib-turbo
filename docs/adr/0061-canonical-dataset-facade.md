# ADR 0061: Use one native Dataset facade

Status: Accepted for source wiring; coordinated installed-artifact acceptance pending.

Supersedes ADR 0009's temporary retention of the older Python Dataset wrapper.
The storage, transaction, scope and report-acceptance contracts remain binding.

Export the existing StoreDataset class under Dataset as well. Both spellings
identify the same RDFLib Dataset subclass and existing native Store owner.
Remove the older Python wrapper rather than maintaining two Dataset contracts.
GraphOperations remains shared by Store and Transaction; its separate native
operations are not removed by this facade change.

Public Dataset inplace validation follows ADRs 0043 and 0050: it borrows a
transaction and rejects a second writer while report acceptance is pending.
Callbacks may read committed data, but cannot independently commit writes
through that Store. Rejection rolls back private rule output and releases the
writer, allowing subsequent writes. ADR 0044's optimistic acceptance and
surviving independent callback writes describe the old internal NativeDataset,
not this public transactional owner.

The SHACL facade uses the existing borrowed native graph protocol for validation
and rules. Remove the older NativeDataset assignment path. Native graph views,
physical default/named/blank contexts, namespace metadata, and transaction owners
retain their existing identities. No serialization or reference Memory store
may prepare a native input for inference or SHACL execution.

Ordinary RDFLib OWL input is explicitly imported once into an unpublished native
Store using typed bounded records. The private inference capture returns only
changed records with owning lexical leases. Apply removals and additions to the
original physical contexts, then close both leases. Do not copy the whole closure
back or build an intermediate indexed additions store for this writeback path.
Caller Store and persistent capture restrictions remain explicit.

Public infer_delta deliberately requests an independent additions Dataset. Its
materialization, namespace preparation and revision check must finish before
accepting the caller's native inference transaction. A decoding, callback,
metadata or materialization failure aborts pending inference. A committed private
OWL capture cannot implement this public acceptance contract.
Discard the public result's unrequested removals explicitly through native
changed identities. Do not restore removed input terms just to discard them or
emit their factory callbacks and warnings. Private OWL writeback still consumes
the actual bounded removal rows.

Programmatic full-Term observations use logical RDF term roles and physical
contexts, independently of syntax serializer limitations. Preserve empty graph
domains, explicit datatype state and graph-identifier kind. Strict parser and
serializer contracts remain separate. Bounded dataset union and quad readers
must avoid whole-dataset JSON materialization and Python grouping dictionaries.
Committed Store cursors own their selected snapshot, including each lazy per-SPO
context cursor. Transaction readers retain their existing pending-state contract;
a bounded transaction iterator is separate work, never a committed-reader
substitution.

Separate ordinary Graph destinations, raw language-tag presentation, complete
original import compatibility and unsupported backend operations remain parity
work. This consolidation alone establishes neither full drop-in parity nor a
performance improvement. Verification requires original source archives and
installed wheels of all three facades, complete payload and engine identities,
and exact original-pipeline observations before performance measurement.
