# ADR 0072: Elide reflexive subproperty witnesses

Status: Accepted

The fixed Rdfs7 catalogue clause repeatedly joins existing assertions with
reflexive subproperty edges. Its head is identical to its positive assertion
body fact whenever the source and target predicates are the same RDF term.
`Pending::push_head` already refuses that head from the read union, including
when the publication destination is a separate graph.

Add exact-term inequality between those two predicate variables when compiling
this specific catalogue ID. Derive the operands from the published typed head
and body atoms. Check relations, arities, the subPropertyOf predicate, matching
subject/object positions, schema endpoints and four distinct variable roles.
Reject an unexpected definition before compilation. Generic rule compilation
retains the unmodified clause; no shared definition or arbitrary rule is rewritten.

Use the existing BuiltinFilter::Ne lowering to !sameTerm. Value inequality would
discard distinct RDF terms with equal values and is not permitted. Preserve
ordinary and generalized terms, immutable round visibility, fixed catalogue
order and grouping, source contexts, destination publication and rollback.

The isolated regression first reproduced 1,024 redundant heads and 1,734,143
requested allocation bytes for an unchanged relation. Keep the allocation budget
and the non-reflexive publication/rollback control as regressions. Installed-wheel
whole-output comparisons and measured allocation profiles remain required before
claiming a pipeline speed or memory improvement.
