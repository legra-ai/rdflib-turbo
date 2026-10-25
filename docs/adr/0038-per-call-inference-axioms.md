# ADR 0038: Initialize optional inference axioms per invocation

Status: Accepted

Expose `axiomatic_triples` and `datatype_axioms` as independent keyword-only
booleans on Dataset, Store and Transaction inference, including Dataset delta
inference. Both default to false. Carry a typed immutable options value through
the transaction owner; no process-wide reasoner configuration is introduced.

Initialize the selected RDFS and/or OWL-RL axiom tables before existing one-time
rules and cyclic inference. The four finite tables are derived from the pinned
OWL-RL 7.6.2 constants, with source provenance and its original license retained.
No Python reasoner runs in the native implementation. This does not implement
OWL-RL's separate global datatype conversion hooks.

RDFS container axioms use the reference constructor's initial logical-graph
enumeration: start at canonical `rdf:_1`, advance once per distinct matching
triple, and stop at the first empty predicate. This includes the reference's
behavior for repeated container predicates and gaps. Storage union reads can
repeat a triple across contexts; an exact indexed probe chooses one snapshot
representative without retaining a predicate-sized deduplication set. Memory
storage iteration order is not assumed to be sorted.

Explicit optional axioms belong to the destination even when an identical SPO
already exists in another source context. Named Dataset inference therefore
streams these initializer facts into the actual destination journal after
successful reasoning, in addition to the union journal's new entailments.
Delta results contain exactly the new destination quads. No original source
contexts move. All optional initialization and inferred writes share the existing
atomic journal or owning storage transaction; later failures publish nothing.

The native RDF model excludes literal subjects. Differential tests explicitly
compare the ordinary-RDF projection of all three reference semantics and every
flag combination. They retain standard-RDF diagnostics and blank nodes, verify
destination/delta behavior, memory and durable union container enumeration, and
rollback after an error in a later inference round. This is not a claim of full
generalized-RDF or complete reasoner parity.

Scoped Graph and StoreDataset inference forwards the same typed per-call flags
and caller-owned diagnostic history. Scoped sources initialize options through
the existing configured expansion path; separate destinations receive axioms
without becoming rule sources. Transaction failure, caller Decimal status
publication and provisional diagnostic metadata retain their existing boundaries.

## Full-Term datatype subjects

Implicit datatype membership belongs to the original literal term, including
its exact lexical form and explicit datatype. Emit that membership and its
ancestor types in the one-time phase. Plain and language-tagged RDFLib literals
have no explicit datatype for this purpose. Explicitly typed source subjects
receive the reference's ancestor types without copying their existing base type
to a separate destination.

Keep sameAs aliases distinct from implicit literal subjects. OWL7.6.2's explicit
map also classifies such an alias as rdfs:Datatype and emits its ancestor types;
it does not directly copy the literal's base datatype membership to the alias.
Synthetic original one-time probes and native regressions preserve these roles,
including explicit xsd:string, integer/decimal ancestry, plain/language controls
and separate-destination behavior.

## Full-Term initial resource membership

RDFS cycle-one `rdfs4b` seeds every original object as `rdfs:Resource`, including
plain, language-tagged and explicitly typed literals. Preserve the full term in
this initializer independently of OWL's explicit-datatype membership boundary.
Combined mode then derives the corresponding `owl:Thing` membership. This does
not make later-round terms part of the initial resource domain or change strict
RDF parsing and public SPARQL projection.

Complete synthetic OWL7.6.2 closures for four literal kinds and all three modes
compare both GeneralizedDataset and Store backends without filtering generalized
subjects. The plain/language combined controls expose the former missing
resource/Thing facts that datatype initialization happened to hide for numeric
literals.
