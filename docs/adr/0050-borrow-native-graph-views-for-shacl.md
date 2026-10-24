# ADR 0050: Borrow native Graph views for SHACL

Status: Accepted

Graph and StoreDataset implement the same source context used by the native
Store and Transaction validation adapters. A Graph borrows only its selected
physical context through the existing engine ScopedGraph, exposing that context
as the logical default graph. It cannot read unrelated or named contexts through
SHACL patterns or SPARQL. StoreDataset retains SHACL dataset-union semantics,
independently of its default_union query option.

Carry the optional typed graph scope through the existing validation operation.
Keep derived-only rule output private until report decoding and warning policy
succeed. Map logical rule additions to the selected physical graph at acceptance;
do not build a second remapped dataset. Dataset additions use the default graph.
No Python source traversal, document serialization, base clone, independent
storage owner or new engine implementation is involved.

Graph validation reuses its existing native transaction and staged namespace
mapping. After caller argument normalization, namespace mutation attempts advance
a mapping-local revision, including no-op binds. Source contexts check that
revision before accepting report output;
a change aborts pending RDF and rule writes. Namespace changes to the original
facade during an implicitly owned validation are checked before native commit.
This complements the native RDF mutation revision in ADR 0043.

Committed non-inplace validation retains the pinned MVCC read implementation and
does not reserve a writer. Explicit transaction validation reads pending changes,
leaves derived output private during Python report handling, and accepts it on
the same owner. All existing rollback and report-rejection rules remain binding.

The internal protocol remains 1. The engine source and feature profile do not
change: this connects existing engine and storage capabilities at their missing
facade boundary. Extracting a separate Python distribution does not repair that
boundary and is not required by this decision.
