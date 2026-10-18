# ADR 0009: Reuse the RDFLib facade over native transactional storage

Status: Implemented locally; not released

The released package exposed `rdflib.Graph` as an incidental import. A downstream
experiment could therefore appear successful while allocating the reference
Memory store. That does not demonstrate native replacement compatibility.

Expose `rdflib_turbo.Graph` as a real subclass of RDFLib's Python Graph facade,
with an explicit `rdflib_turbo.graph.RDFLibStore` adapter. Every statement lives
in the existing exact-term native Store. Pattern scans, mutation, graph names,
counts and SPARQL run against that Store or its transaction owner. No reference
Memory graph, indexed Dataset copy, serialization adapter or Python SPARQL
processor is used. A requested CONSTRUCT/DESCRIBE result may own new native
storage containing only its output, as distinct from copying input for evaluation.

Reuse RDFLib terms, NamespaceManager, serialization and graph utility algorithms.
Parse supported RDF documents natively, preserving literal lexical spelling and
giving each parsed document a fresh blank-node scope. Direct additions transport
typed terms as JSON, rather than reparsing `.n3()`: RDFLib permits relative or
otherwise non-absolute URIRef values, and storage must retain the supplied term.
Native RDF document parsers retain their own syntax validation. Subject/predicate
term kinds still enforce the ordinary-RDF boundary in ADR 0001.

Graph views have no independent data owner. Explicit transactions create views
of the same native transaction and commit only after successful context exit.
Native operation errors abort the session, including errors caught by Python.
Namespace bindings are shared Store-facade metadata and staged together with a
Graph transaction; no persistence of namespaces is claimed. Native parsing stages
quads and namespace bindings and never commits a partial document. Scoped counts
execute natively without materializing Python triple results.

Expose `StoreDataset` for RDFLib context/quad and transaction contracts. Retain
the existing public `Dataset` unchanged: ADRs 0006/0007 prohibit adopting the
Store in inference and SHACL before direct adapters exist. Graph/StoreDataset
advertise `_turbo_native_graph = True`; converting them through
`Dataset.from_rdflib` is explicitly rejected so downstream inference facades
cannot accidentally hide an input copy. This slice does not claim Store-based
inference or SHACL integration.

Override Graph query/update entry points so RDFLib cannot catch an unsupported
Store operation and execute its Python evaluator. Prepared Python queries,
initial bindings, alternate processors, plugin stores and scoped named-graph
SPARQL Update fail explicitly until implemented. Unchanged original imports and
package ownership remain separate unfinished work.

Verification uses installed wheels, checks origin/native class provenance,
prohibits reference Memory-store allocation and Python SPARQL evaluation, checks
exact terms/contexts and commit/abort semantics. The first downstream experiment
runs the ten original rdfpandas tests with two explicit Graph import changes in
a separate checkout; it is an API migration experiment, not unchanged-import
compatibility or evidence of a performance improvement.
