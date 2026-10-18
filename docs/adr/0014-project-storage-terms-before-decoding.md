# ADR 0014: Project statement positions before storage decoding

Status: Accepted

Graph-scoped Python reads request only the RDF terms needed by their caller. The
original native QuadIter still decoded subject, predicate, object and graph before
the binding discarded unrequested values. Large object literals were therefore
looked up and allocated even for subject-only scans.

Add a consuming QuadIter.project(subject, predicate, object) adapter in the existing
vendored Oxigraph store module. ProjectedQuadIter retains the original encoded
cursor and reader, including its owned snapshot or transaction borrowing lifetime.
It decodes only selected statement positions and never decodes graph names. The
native graph scope is already established by the source pattern. No additional
snapshot, base copy, raw storage getter, result collection or term cache is added.

The result has three optional term slots. Selecting no slots still produces one
result per matching statement, supporting membership checks. Storage errors and
decoding errors for requested terms propagate. The change does not group contexts,
implement union distinctness or change transaction ownership.

Standalone backend regressions exercise all eight projection masks, exact literals,
blank nodes, default/named scopes, owned snapshots and borrowed transaction views,
including a large unrequested object. Installed-wheel workload benchmarks quantify
performance; semantic tests alone do not claim an allocation or RSS reduction.
