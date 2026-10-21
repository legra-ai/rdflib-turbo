# ADR 0039: Borrow graph scopes for native inference

Status: Accepted

Store inference retains ADR 0008's union semantics. Add direct inference on its
RDFLib Graph and StoreDataset facades without converting either to the older
Dataset owner or serializing input triples.

A Graph reads exactly its native graph scope, including a default graph or a
blank-node graph identifier. A StoreDataset reads the union. New statements go
to the supplied destination or the caller's default destination. The native
boundary transports graph identifiers through the existing typed graph encoding;
it does not reinterpret blank nodes or relative identifiers as IRI strings.

The existing transaction-union backend remains unchanged. A scoped backend
restricts indexed matches and prepared query evaluation to one source graph.
When the destination differs, it checks both source and destination for existing
facts but only reads the source for rule evaluation. This matches the original
reasoner's separate Graph destination behavior: written consequences terminate
duplicate production without feeding back into the source snapshot.

Reuse the Store's operation journal and owner session. Inference sees preceding
transaction changes, commits only at the existing boundary and aborts all pending
writes on failure. It adds no base snapshot or Python triple materialization.
Python destination validation also aborts an active transaction on error.

This direct provider is a prerequisite for owlrl-turbo's original in-place class
interface on native Store graph views. SHACL's Store adapter and nonmutating
functional inference on these views remain separate integration work; their
absence must not be hidden by whole-dataset conversion.
