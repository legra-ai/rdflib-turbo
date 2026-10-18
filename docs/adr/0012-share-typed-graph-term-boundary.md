# ADR 0012: Share typed Graph term validation for reads and writes

Status: Accepted

Graph.add and graph-scoped committed-store scans transport RDF terms as typed
four-field tuples: kind, lexical spelling, optional datatype and optional language.
The graph argument is a URI or blank-node tuple; None identifies the default graph.
This boundary does not serialize RDF documents or JSON, normalize literal spelling,
or require absolute URIRef values.

One native typed boundary validates term kinds and statement positions for both
scans and insertions. Remove the displaced JSON insertion methods rather than
retaining a second input route. Existing transaction and union read contracts are
unchanged by this insertion change.

Explicit transaction insertion transports the tuples to the transaction owner
before validating them. Invalid RDF terms therefore abort the transaction and all
prior staged statements even when Python catches the operation error. Implicit
insertion validates and commits within its existing inline transaction. Neither
path copies the base graph or creates a second dataset.

Native tests verify exact literals, relative IRIs, named/default scopes and invalid
operation rollback; Python tests prohibit JSON transport on Graph.add and exercise
both committed-store and explicit transaction views.
