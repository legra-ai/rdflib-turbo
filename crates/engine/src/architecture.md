
## Borrowed SPARQL graph architecture

[`GraphRead`] is the storage boundary: it streams indexed quad matches, enumerates
named graphs and checks graph existence. Implementations for an in-memory
`oxigraph::model::Dataset` and an open `oxigraph::store::Transaction` preserve
their graph contexts. Transaction reads include pending insertions and deletions.

[`UnionGraph`] optionally exposes distinct data triples in the default graph while
preserving named contexts. [`OverlayGraph`] optionally adds a borrowed shapes
union under a fresh named graph, including when shapes are empty. For SHACL, wrap
the data union in the shapes overlay, in that order. Shapes stay outside the data
union and remain available to queries through the reserved shapes graph name.

[`QueryGraph`] implements `spareval::QueryableDataset` for these borrowed sources.
The existing SPARQL evaluator retains parsing, planning, joins, expressions and
property paths. The adapter translates graph matching: an absent [`GraphRead`]
graph filter includes all contexts, whereas a SPARQL wildcard graph pattern
matches named graphs only. Named-graph metadata retains empty native and overlay
graphs. Native readers retain every stored term kind in subject and predicate
positions; only graph names have the named/blank/default domain. The legacy
strict syntax Dataset cannot contain generalized positions, so those bound
patterns have no matching row in that adapter.

A source must stay consistent while results are consumed. Rust lifetimes retain
its borrow; temporary match terms need not outlive the returned stream. Read and
metadata errors remain errors, including during lazy query-result iteration.
The adapter preserves stored RDF terms; it cannot restore lexical forms already
normalized by a backend.

The adapter and views do not copy the base dataset or collect all graph names.
The evaluator's query-dependent joins, sorting, aggregates and output can still
consume memory. Transaction ownership and commit/rollback remain with the caller.

[`IndexedShapesSource`] owns an existing [`oxigraph::store::ReadSnapshot`]. A
selected physical graph is exposed as the sole logical default graph; an absent
scope exposes the union plus the actual named graphs. The same scoped/union
matching implementation is used by the borrowed views. Keeping the original
snapshot preserves its terms and version after the originating Store is changed
or closed, without copying its dictionary or rebuilding its indexes.

SHACL targets, constraints, reports and scoped function callbacks use these
borrowed views. `CompiledShapes::validate_transaction` runs rules and validation
against the caller's pending writes; it never commits. Drop the transaction on
an execution error or an unwanted report, and commit explicitly on acceptance.
`validate_staged` and `validate_delta` keep only rule additions over a borrowed
base. `expand_store` returns those additions, rather than the full dataset.

The Python Store commit gate validates after the transaction body, constructs
the report, then commits only on conformance. Nonconformance and exceptions abort
all pending writes, including inference and SHACL rule additions. Scoped function
callbacks borrow the same phase view; they do not clone the base dataset.
Target sets, per-rule results, reports and query intermediates can still grow
with the validation workload; avoiding base snapshots does not bound those costs.

### Query pending changes through the composed views

```rust
use pyshacl_turbo_engine::{OverlayGraph, QueryGraph, UnionGraph};
use pyshacl_turbo_engine::oxigraph::{
    model::{Dataset, NamedNode, Quad},
    sparql::{QueryResults, SparqlEvaluator},
    store::Store,
};

let store = Store::new()?;
let mut transaction = store.start_transaction()?;
transaction.insert(&Quad::new(
    NamedNode::new("urn:alice")?,
    NamedNode::new("urn:knows")?,
    NamedNode::new("urn:bob")?,
    NamedNode::new("urn:data")?,
));

{
    let shapes = Dataset::new();
    let data_union = UnionGraph::new(&transaction);
    let view = OverlayGraph::new(
        &data_union, &shapes, NamedNode::new("urn:shapes")?.into(),
    )?;
    let result = SparqlEvaluator::new()
        .parse_query("ASK { <urn:alice> <urn:knows> <urn:bob> }")?
        .on_queryable_dataset(QueryGraph::new(&view))
        .execute()?;
    assert!(matches!(result, QueryResults::Boolean(true)));
}

// Dropping the transaction aborts its pending changes.
drop(transaction);
assert!(store.is_empty()?);
# Ok::<(), Box<dyn std::error::Error>>(())
```

### Validate before committing

```rust
use pyshacl_turbo_engine::{CompiledShapes, IndexedShapesSource, Options};
use pyshacl_turbo_engine::oxigraph::{io::RdfFormat, store::Store};

let shapes = IndexedShapesSource::parse_graph("", RdfFormat::Turtle, None)?;
let program = CompiledShapes::compile(shapes, false)?;
let store = Store::new()?;
let mut transaction = store.start_transaction()?;
let report = program.validate_transaction(&mut transaction, Options::default())?;
if report.conforms {
    transaction.commit()?;
} else {
    drop(transaction);
}
# Ok::<(), Box<dyn std::error::Error>>(())
```
