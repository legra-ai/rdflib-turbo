# rdflib-turbo

[![PyPI](https://img.shields.io/pypi/v/rdflib-turbo)](https://pypi.org/project/rdflib-turbo/)

[![Tests and wheels](https://github.com/legra-ai/rdflib-turbo/actions/workflows/ci.yml/badge.svg)](https://github.com/legra-ai/rdflib-turbo/actions/workflows/ci.yml)

[![Python line coverage](https://legra-ai.github.io/rdflib-turbo/python-lines.svg)](https://legra-ai.github.io/rdflib-turbo/)
[![Python branch coverage](https://legra-ai.github.io/rdflib-turbo/python-branches.svg)](https://legra-ai.github.io/rdflib-turbo/)
[![Rust line coverage](https://legra-ai.github.io/rdflib-turbo/rust-lines.svg)](https://legra-ai.github.io/rdflib-turbo/)
[![Rust region coverage](https://legra-ai.github.io/rdflib-turbo/rust-regions.svg)](https://legra-ai.github.io/rdflib-turbo/)
[![Rust function coverage](https://legra-ai.github.io/rdflib-turbo/rust-functions.svg)](https://legra-ai.github.io/rdflib-turbo/)

Functional alpha of a native RDF dataset runtime for Python, built on Oxigraph
and exact-term compatibility work extracted from pyshacl-turbo.

Companion turbo projects: [pyshacl-turbo](https://github.com/legra-ai/pyshacl-turbo) and
[owlrl-turbo](https://github.com/legra-ai/owlrl-turbo).

```sh
uv add rdflib-turbo==0.2.0a1
```

```python
from rdflib_turbo import Dataset

# The data stays native between parsing, querying and owlrl-turbo inference.
data = Dataset().parse(data='<urn:alice> <urn:name> "Alice" .')
for row in data.query('SELECT ?name WHERE { <urn:alice> <urn:name> ?name }'):
    print(row.name)
print(data.serialize(format="nquads"))
```

The alpha provides native parsing and serialization, SELECT/ASK/CONSTRUCT/DESCRIBE,
exact literal spellings and datatype identity, named graphs, independent copies,
and explicit RDFLib conversion. Query results are RDFLib Result objects; returned
terms are Python RDFLib objects. Native operations release Python's GIL. A lock
covers each complete native read or write. Parsing and inference commit only on
success. Parsing separate documents gives blank nodes separate scopes.

Supported native format names: turtle, nt, nquads, trig, xml, n3, json-ld.
Dataset and Store accept UTF-8 text, bytes or a local path; their source URL and
file-object inputs are not implemented.
Default-graph querying is standard; request default_union=True for a union view.
Graph-only serializers reject named-graph data rather than discarding contexts.

This is **not a complete drop-in RDFLib replacement**. It uses the rdflib_turbo
namespace. Dataset has inference/SHACL integration; Store has persistence,
SPARQL Update, direct inference and explicit transactions. The unreleased Graph
and StoreDataset APIs add RDFLib facades over that native Store. Arbitrary RDFLib
storage plugins, initial query bindings, custom functions and remote SERVICE
execution are not provided. RDFLib conversions
copy data and explicitly preserve caller blank-node identifiers. Query results are
eagerly materialized. No performance or complete-conformance claim is made.

## RDFLib Graph and dataset facades

Graph and StoreDataset are currently unreleased source APIs. Import them explicitly
from rdflib_turbo; importing rdflib continues to select the original RDFLib classes.

| API | Data owner and intended use |
|---|---|
| Dataset | Native dataset with the shared companion inference and SHACL adapters |
| Store | Native memory or persistent storage, SPARQL Update and scoped transactions |
| Graph | RDFLib Graph facade over one native Store graph |
| StoreDataset | RDFLib Dataset facade with default and named views of one native Store |

```python
from rdflib import Literal, URIRef
from rdflib_turbo import Graph, StoreDataset

alice, name = URIRef("urn:alice"), URIRef("urn:name")
graph = Graph()
graph.add((alice, name, Literal("Alice")))
assert list(graph.objects(alice, name)) == [Literal("Alice")]
assert graph.query('ASK { <urn:alice> <urn:name> "Alice" }').askAnswer

dataset = StoreDataset()
dataset.default_graph.add((alice, name, Literal("default")))
named = dataset.graph(URIRef("urn:people"))
with dataset.transaction() as pending:
    pending.graph(URIRef("urn:people")).add((alice, name, Literal("Alice")))
    assert len(named) == 0              # Other views see committed state
assert list(named.objects(alice, name)) == [Literal("Alice")]
```

These facades reuse RDFLib terms, namespace management, serializers and graph
utility algorithms. Statements, pattern matching, counts and SPARQL remain in
native storage; no reference Memory graph or Python SPARQL evaluator runs.
Ordinary scoped Graph iteration reads bounded native batches. Dataset-wide and
explicit-transaction iteration, query results and serialization can still
materialize larger results.

StoreDataset graph views share their native backend and namespace bindings.
Queries use the default graph unless StoreDataset(default_union=True) is selected;
a named Graph query uses its own graph as the query's default graph. Dataset-level
queries can address named graphs with GRAPH. Graph(store=store, identifier=...)
and StoreDataset(store=store) also accept an existing native Store, including a
persistent Store. Namespace bindings are facade metadata and are not persisted.

Use Graph.transaction() or StoreDataset.transaction() as a context manager.
Pending views see the same transaction's writes; normal exit commits statements
and namespace changes together, while exceptions abort them. Native operation
failures abort even when caught inside the scope. Nested transactions and manual
commit()/rollback() on the RDFLib adapter are unsupported. Parsing is atomic and
uses RDFLib input-source handling followed by the native parser; parser-specific
keyword options are rejected.

SPARQL requires query/update strings and the native processor. Prepared RDFLib
queries, nonempty initBindings, alternate processors and scoped named-Graph
SPARQL Update are rejected. Use StoreDataset.update() for dataset updates,
including explicit GRAPH clauses. Quoted graphs and arbitrary RDFLib store
plugins are unsupported. See the [facade decision](docs/adr/0009-native-rdflib-graph-facade.md)
and [bounded read contract](docs/adr/0011-bounded-typed-scoped-reads.md).

## Transactional storage

```python
from rdflib_turbo import Store

with Store() as store:                 # In-memory default
    with store.transaction() as tx:
        tx.parse(data='<urn:alice> <urn:name> "Alice" .')
        tx.update('INSERT DATA { <urn:alice> <urn:active> true }')
        assert len(tx) == 2             # Sees this transaction's writes
        assert len(store) == 0          # Readers see committed state
    assert len(store) == 2               # Committed at normal scope exit

with Store("./database") as store:     # Persistent RocksDB directory
    store.update('INSERT DATA { <urn:alice> <urn:name> "Alice" }')
```

Exceptions abort the transaction. A native operation failure also aborts it if its
exception is caught inside the scope. Transaction handles become unusable after
exit. Parsing and updates outside a scope are individually atomic. Another writer
or close on the same handle fails immediately while a transaction is active.
Store queries use the default graph unless default_union=True is requested.
Named graphs and exact literal spelling/datatype identity survive commit and reopen.

Python wheels include RocksDB support; Store() still defaults to memory. Rust-only
builds enable persistence with `--features persistence`. Closing a store releases
its database handle. Store and transaction inference use the existing rdfs, owlrl
and both compatibility modes directly on the native transaction:

```python
with Store() as store:
    with store.transaction() as tx:
        tx.parse(data="<urn:Child> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:Parent> . <urn:alice> a <urn:Child> .")
        tx.infer(inference="rdfs", destination="urn:inferred")
        assert tx.query("ASK { GRAPH <urn:inferred> { <urn:alice> a <urn:Parent> } }").askAnswer
```

Inference reads the union of graph contexts and writes only new facts to the
requested destination (the default graph when omitted). It sees pending updates;
errors abort the entire transaction. Standalone store.infer(...) is atomic.
See the [direct inference contract](docs/adr/0008-direct-transaction-inference.md).
Dataset, Store and Transaction also accept `axiomatic_triples=True` and
`datatype_axioms=True` on `infer`; Dataset supports these flags on `infer_delta`
as well. Both flags default to false and apply only to that invocation. They add
the selected reference RDFS/OWL-RL axiom tables before reasoning. Native results
use ordinary RDF: literal subjects and global Python datatype-conversion hooks
are not supported. See the [axiom options contract](docs/adr/0038-per-call-inference-axioms.md).
The low-level Store has no namespace-binding API. Graph and StoreDataset
provide namespace management through their RDFLibStore adapter, without
registering a named RDFLib storage plugin.
Compiled pyshacl-turbo shapes validate Store and open Transaction directly;
stored data is never copied into Dataset for inference or validation.

```python
from pyshacl_turbo import compile_shapes
from rdflib_turbo import Store, ValidationError

shapes = compile_shapes('''@prefix sh: <http://www.w3.org/ns/shacl#> .
  <urn:S> sh:targetNode <urn:s>;
    sh:property [sh:path <urn:name>; sh:minCount 1].''')
with Store("./database") as store:
    with store.transaction(shapes=shapes, iterate_rules=True) as tx:
        tx.parse(data='<urn:s> <urn:name> "Alice".')
        tx.infer(inference="rdfs")
```

The gate runs after the body and commits only after a conforming report has been
constructed. Nonconformance raises ValidationError carrying report and text,
and aborts all pending writes, inference and rule additions. Exceptions abort
the same owner; caught native execution or report-processing errors leave the
transaction closed. Direct shapes.validate(tx) returns a report and lets the
caller choose acceptance. Companion wheels must contain identical engine sources.

![Borrowed reads and transaction validation commit gate](https://raw.githubusercontent.com/legra-ai/pyshacl-turbo/main/crates/engine/src/architecture.svg)

Oxigraph buffers the transaction delta in memory. The base dataset is not copied
for rollback; transaction changes, parsed input documents and materialized query
or serialization results can still be large. See the
[storage decision](docs/adr/0006-exact-term-transactional-store.md) and
[transaction ownership contract](docs/adr/0007-store-api-and-scoped-transactions.md).

## Inference and SHACL

```python
from owlrl_turbo import expand
from pyshacl_turbo import validate

expand(data, inference="rdfs", inplace=True)
conforms, report, text = validate(data, shacl_graph="shapes.ttl")
```

This shared companion path uses Dataset, Store or Transaction, not Graph or StoreDataset. Store-backed
facades can invoke direct inference through their native store.backend, including
inside a transaction; pass their native backend to compiled SHACL shapes. Converting
them through Dataset.from_rdflib is explicitly rejected to prevent copying input.

The shared validation path requires this runtime's 0.2.0a1 release, owlrl-turbo
0.1.0a3 or later, and the companion pyshacl-turbo shared-dataset release. Parsing,
in-place inference and ordinary validation retain the same native data object.
Only the validation report becomes an RDFLib graph. Rules stage changes privately
and commit only after success when inplace=True is requested. See
[the SHACL ownership ADR](docs/adr/0004-dataset-owned-shacl.md),
[the runtime ADR](docs/adr/0001-shared-native-runtime.md) and
[source provenance](vendor/README.md).

Requires Python 3.11+. macOS wheels require macOS 11 or later on Intel and ARM64.
Source builds require Rust 1.99.1, a C++20 compiler and libclang. Both project code and
extracted inference code are MIT OR Apache-2.0; vendored sources retain licenses.
Copyright 2026 DataRoad Inc, Delaware, USA, trading as Legra.

## Development

Development uses native uv project commands and the committed `uv.lock`.
Run `make dev` to sync the project and test tools, then `make check`. Add or update
Python dependencies with `uv add`; run Ruff, mypy and pytest through `uv run`.
Build wheels and source distributions with `uv build`. The coverage and release
harnesses create isolated uv projects and install the exact built wheels.

The [native heap profiler and allocation budget](https://github.com/legra-ai/rdflib-turbo/blob/107a382acb42282069f5be3a58a51f687ef3e14e/docs/inference-memory.md) explain
phase-level memory measurements and the inference insertion journal.

The [shared companion-project benchmark suite](https://github.com/legra-ai/pyshacl-turbo/blob/main/benchmarks/companions.md)
compares parsing, serialization, queries, inference, conversion costs and complete
SHACL pipelines against the reference packages. With all three repositories as
siblings, run `make benchmark-companions`. It uses pinned published wheels and
separate timing and peak RSS workers; it does not use local editable builds.

Use `make dev` and `make check`. The initial alpha is published only by the
verified GitHub Actions workflow; see [release instructions](docs/releasing.md).

```sh
make dev
uv run --locked --extra test ruff check python tests scripts
uv run --locked --extra test pytest
cargo fmt --check
cargo clippy -- -D warnings
```

### Coverage

Coverage badges update from successful main-branch CI measurements, including the
daily scheduled run. Python lines and branches are separate; Rust badges measure
owned native source exercised by Rust and Python integration tests.
Rust branch coverage and dependency/vendored-patch coverage are not measured by these badges.

CI requires 100% measured Python package line and branch coverage. Coverage is
separate from full API compatibility and standards conformance.
