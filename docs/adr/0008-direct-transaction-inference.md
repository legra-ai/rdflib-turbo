# ADR 0008: Run inference on the active native transaction

Status: Accepted

Store and Transaction expose infer(inference="rdfs" | "owlrl" | "both",
destination=None), sharing the existing Dataset rule tables, initialization and
fixed-point loop. These are the existing Python compatibility modes, not a new
claim of standards completeness or adoption of the portable preset catalogue.

The engine reads through a small internal capability boundary: indexed quad
matches, prepared SPARQL evaluation, membership and insertion. The Dataset
insertion journal and native Store transaction implement that boundary. Neither
copies the Store into a Dataset. Native queries use the transaction's graph union;
indexed traversal matches all contexts. Duplicate copies of a list edge in
several contexts represent one union edge; distinct first/rest values still fail.
Only absent union facts enter the per-round delta. New facts go to the requested
IRI graph, or the default graph when omitted. Original statements keep their
contexts and are never projected into the destination. Subsequent rounds see
inferred facts in any destination through the same union.

Stream property-chain starts and key candidates through indexes. Do not retain
all subjects, class instances or candidate intersections. Rule lists and their
cycle-detection state scale with the individual schema expression, and pending
round additions scale with the inference delta. SPARQL evaluation and the native
transaction delta can still allocate memory proportional to their results. This
is not a bounded-memory inference-closure guarantee.

Implicit Store inference commits only on success. Explicit transaction inference
sees preceding writes and leaves successful additions pending until scope exit.
Any inference error, including invalid mode/destination or a malformed RDF list,
terminates the owner thread and aborts the entire transaction, including preceding
updates and successful inference calls. No base snapshot or inference-specific
rollback journal is added to Store. Other readers see only committed data.

Dataset keeps its current ownership and named-graph working-view implementation.
SHACL and namespace persistence remain separate prerequisites for Dataset's final
Store adoption. The canonical SHACL engine must gain direct backend support;
copying a Store for validation remains prohibited by ADR 0006.
