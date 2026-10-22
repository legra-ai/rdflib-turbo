# ADR 0035: Return native inference additions before RDFLib writeback

Status: Accepted

`Dataset.infer_delta(inference="owlrl", destination=None)` mutates its receiver
with the same closure and graph-scope semantics as `Dataset.infer`, then returns
a separate native Dataset containing only exact quads newly added by that call.
The result copies namespace bindings and preserves output graph contexts. Repeated
inference on a saturated dataset returns an empty result. This API does not
change the supported modes or standard RDF output boundary.

Read additions from oxrdf's existing insertion journal. Default-graph inference
does not take a before/after snapshot, clone the original dataset, or build a
second index of original statements. Stage the returned dataset before committing
the receiver's transaction. Invalid arguments and inference failures retain the
existing all-or-nothing contract. Named-graph inference borrows a union view of
the original dataset and retains only staged additions. It must not clone the
base or duplicate all original triples into a working default graph. An existing
destination insertion journal identifies actual newly added quads without
exporting original contexts.

The borrowed view preserves original named contexts and exposes one exact SPO
representative in its logical default graph. Staged output is visible to later
rounds without double-counting triples already in that union. Direct RDFS
literal publications and optional initialization axioms check the physical
destination: facts present only in another context may still require output.
Only successful inference publishes the retained additions, through the existing
insertion transaction. An isolated allocation budget catches source-sized
lexical copies even when the invocation produces no output.

Keep stored query terms borrowed through joins. The pinned dataset's unscoped
indexes group each exact SPO before graph names, allowing union deduplication
with one previous borrowed triple. Classify output once on insertion: genuinely
new facts participate in later reads; publications already in the source union
only need destination writeback. These sets are disjoint and retain no duplicate
base index. Boolean membership uses exact indexed probes and must not reconstruct
owned lexical payloads; an isolated allocation budget enforces this separately.

The returned dataset owns the derived output and therefore consumes memory
proportional to that output. This is not a bounded-memory materialization claim.
RDFLib adapters can convert this delta and write only additions, avoiding term
allocation and insertion for the original graph. Nonmutating adapters still
return the complete closure. Companion adapters require a released runtime that
provides the new API before publishing that dependency requirement.

Ship `py.typed` with inline annotations on the public Dataset API and its inherited
parse/query operations. The marker is verified from installed release wheels.
It does not imply every native Graph/Store operation has a complete static typing
contract or that typing is equivalent to inference conformance.
