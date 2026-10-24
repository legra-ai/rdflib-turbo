# ADR 0037: Validate SHACL inside the storage transaction

Status: Accepted

Extend ADR 0007's owner-thread operation protocol with native SHACL validation.
Compiled metadata is shared through Arc; the operation borrows the existing
Oxigraph transaction. Targets, constraints, functions and reports use indexed
GraphRead views; rules write additions to the transaction or a private delta
for non-inplace validation. No base dataset snapshot or RDF serialization is
permitted at the input boundary.

Store.transaction(shapes=compiled, **options) validates after the user body.
Conformance commits original writes, inference and rule additions together.
Nonconformance raises ValidationError containing the report and aborts. Native
execution errors abort the owner even when caught. Construct the native report
before a native commit, and keep the Python source context open through report
decoding and warning emission so their errors also abort before commit.

Direct compiled.validate(transaction) returns a report, leaving nonconformance
acceptance to its caller. Direct inplace Store validation owns one transaction
through report consumption. Non-inplace Store validation borrows committed state
without reserving a writer; it can run while another explicit writer is open.
Pin one ReadSnapshot for the entire validation. Starting a second write
transaction would deadlock on the in-memory writer lock; taking a fresh read
snapshot for every pattern would permit mixed versions. The public vendor view
exposes indexed reads from one native MVCC version without copying stored quads.
The optional pyshacl-turbo facade is not a runtime dependency. Source adapters
remain internal shared protocol 1, guarded by exact engine fingerprints.

Only the base-copy cost is removed. Native transaction deltas, target sets,
per-rule results, reports and query intermediates can still grow with the work.

Core validation (`advanced=False`) follows pySHACL's inactive-feature policy:
rule bodies, condition/filter references, advanced targets and expression
constraints are read only when advanced mode is enabled. The four core target
predicates still execute normally. Advanced targets alone do not register
their subject as a shape; normal type, core target and shape-reference roles
determine the executable program. Likewise, decode `sh:path` descriptions for
shape subjects rather than unrelated parameter/expression metadata. Enabling
advanced mode still checks and executes accepted features on the same native
owner; ignoring inactive declarations does not enable a Python execution path.
