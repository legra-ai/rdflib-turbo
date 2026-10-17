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

Supported format names: turtle, nt, nquads, trig, xml, n3, json-ld. Input is UTF-8
text, bytes or a local path; source URLs and file objects are not implemented.
Default-graph querying is standard; request default_union=True for a union view.
Graph-only serializers reject named-graph data rather than discarding contexts.

This is **not a complete drop-in RDFLib replacement**. It uses the rdflib_turbo
namespace and deliberately small Dataset and Store APIs. RDFLib Graph/Store
plugins, initial bindings, custom functions and remote SERVICE execution are not
provided. Dataset has inference/SHACL integration; Store has persistence,
SPARQL Update, direct inference and explicit transactions. RDFLib conversions
copy data and explicitly preserve caller blank-node identifiers. Query results are
eagerly materialized. No performance or complete-conformance claim is made.

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
Stores do not yet expose namespace bindings, SHACL or an RDFLib Store plugin.
The SHACL adapter remains next; stored data is never copied into Dataset for inference.

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
