# ADR 0026: Share immutable Dataset records between ordered indexes

Status: Accepted

The Dataset underlying shared parsing, inference and SHACL validation stores each
interned quad key six times in differently ordered BTreeSets. On the measured
64-bit target, each key occupies 112 bytes. Interning lexical values does not
remove this repeated structural payload. This is separate from the MemoryStorage
used by the RDFLib Graph facade.

Store one immutable quad record behind Arc and let each of the six index entries
own a reference. Each index wrapper compares the same four fields in its original
tuple order. Keep canonical GSPO lookup probes on the stack through Borrow; only
that wrapper borrows the record because its ordering matches the record's Ord.
Other index range bounds use temporary owned references that are not retained.
Iteration borrows records without incrementing a reference count per row.

Check duplicates before allocating a record. Removal retains the canonical record
while deleting all six entries. The additive insertion journal owns references to
the same records; rollback removes index and journal ownership before rolling back
the interner. No record contains a reference into an interner or another Dataset.
Dataset clones share immutable records while owning independent trees and cloned
interners with the same encoded keys. Canonicalization rebuilds mapped records;
it must never mutate a key already present in an ordered collection.

Retain exact literal identity, all graph scopes, all sixteen Dataset pattern masks,
all eight Graph masks, original comparator orders, RDF 1.2 nested and directional
terms, generated blank nodes, rollback and clone independence. Run the actual
patched vendor's units and doctests in both RDF modes; reject missing critical test
names instead of accepting Cargo's zero-test success. Forward root dependency
patches and use the vendor lockfile without changing dependency versions.

A separate release-mode allocation budget measures live requested Rust bytes for
10,000 distinct quads. It fails before this change and joins the required isolated
budget runner. This measures retained structural allocations, not process RSS.
Pointer indirection, reference counting and noncanonical range bounds may affect
CPU cost. Installed-wheel query and full three-product CPU/RSS comparisons remain
required before claiming a performance improvement for those workloads.
