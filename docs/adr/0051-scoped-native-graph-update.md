# ADR 0050: Carry graph scope into native SPARQL UPDATE

Status: implemented locally; pending paired source integration.

RDFLib Graph.update selects that graph for unqualified reads and writes. The
native facade previously rejected non-default graph identifiers. Forwarding
those operations unchanged would silently mutate the dataset default graph.

Carry the exact native graph name through Store/Transaction update dispatch to
PreparedSparqlUpdate.with_default_graph. The evaluator selects this implicit
WHERE dataset only when WITH/USING has not selected another dataset, and maps
default output quads and default graph management to the supplied scope. The builder stores only the
current scope; implicit read-dataset selection occurs at execution, so replacing
the scope cannot retain a previous read target. Explicit
GRAPH templates keep their named targets. Graph names may be blank nodes, which
SPARQL AST graph-name types cannot express; never interpolate identifiers or
rewrite query strings to emulate scope.

Execute directly on the existing native readable transaction. Pending writes
remain visible and all statements share the caller's commit/abort boundary.
Caught native errors still abort the whole explicit transaction. StoreDataset
retains whole-dataset UPDATE behavior. No input serialization, dataset mirror,
copy, reference SPARQL processor or fallback is introduced.

The canonical engine and provider must receive identical Oxigraph source bytes
under ADR0048 before paired integration. Changing source identity markers to
conceal a mismatch is forbidden. Protocol remains1.

Verify IRI/blank/default graph scopes against RDFLib, exact initial terms,
statement isolation, pending reads, rollback, caught errors and explicit native
WITH/USING dataset selection. Original Graph dataset-requiring operations,
network LOAD and custom processors are not certified by these bounded tests.
