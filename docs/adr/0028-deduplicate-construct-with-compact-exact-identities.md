# ADR 0028: Deduplicate CONSTRUCT results with compact exact identities

Status: Accepted

CONSTRUCT retained an owned copy of every emitted triple for duplicate detection.
On the inference regression, 7,114 cached triples occupied about 1.65 MB even when
the reasoner had no pending additions. Shrinking Dataset indexes exposed this
remaining overlap: inference exceeded the existing peak-to-retained heap budget.
Boxing pending inference records did not reduce that peak and was rejected.

Assign query-local integer identities to exact terms and retain three integers per
seen triple. Store each distinct dataset InternalTerm once; its trait contract
requires RDF sameTerm equality and hashing. Keep template constants in a separate
dictionary bounded by the template. On first seeing a variable, compare its
external term with template constants so equal terms share identities across
variable and constant origins. Preserve borrowed dataset terms until output
materialization. Direct tuples of wide InternalTerm values increased peak memory
and were rejected.

Do not require a dataset to internalize constants or generated blank nodes: the
previous CONSTRUCT path did not require this. Retain the existing externalization,
subject/predicate validation, unbound-template behavior and per-solution generated
blank nodes. Add result-sized identity state only after validation and the existing
blank-node/quoted-triple cache bypass. Clear variable identities with the existing
result cache; template constant identities remain fixed for the query lifetime.
No global state, lossy fingerprint comparison or inference-only semantic exception
is introduced.

On 64-bit targets retain common result keys as three checked u32 identities.
Use a separate full-width exact set whenever any of the three exceeds u32::MAX;
never truncate an identity or widen all common keys for an exceptional one.
Count both disjoint sets toward the existing limit and clear both together.
32-bit targets keep their existing usize keys. Compare actual requested cache
allocations with the former full-width set; this is separate from process RSS.

Keep the existing strict inference allocation budget unchanged. Persistent tests
cover duplicate solutions, mixed variable/constant templates, exact lexical and
explicit-datatype identity, invalid bindings, generated blank nodes, a backend that
rejects internalization of new nodes, and RDF 1.2 nested/directional terms. CI runs
the actual patched evaluator tests in default, exact-term and RDF 1.2 configurations,
as well as native consumers and the isolated allocation budget. Installed-wheel
pipeline CPU and whole-process RSS remain separate measurements.
