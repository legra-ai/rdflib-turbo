# rdflib-turbo

[![PyPI](https://img.shields.io/pypi/v/rdflib-turbo)](https://pypi.org/project/rdflib-turbo/)
[![Tests and wheels](https://github.com/legra-ai/rdflib-turbo/actions/workflows/ci.yml/badge.svg)](https://github.com/legra-ai/rdflib-turbo/actions/workflows/ci.yml)

Companion turbo projects: [pyshacl-turbo](https://github.com/legra-ai/pyshacl-turbo),
[rdflib-turbo](https://github.com/legra-ai/rdflib-turbo) and
[owlrl-turbo](https://github.com/legra-ai/owlrl-turbo).

Functional alpha of a native RDF dataset runtime for Python, built on Oxigraph
and exact-term compatibility work extracted from pyshacl-turbo.

```sh
pip install rdflib-turbo==0.1.0a2
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
namespace and a deliberately small Dataset API. Graph/Store plugins, SPARQL Update,
initial bindings, persistent storage, namespace extraction from parsed documents,
custom functions and remote SERVICE execution are not provided. RDFLib conversions
copy data and explicitly preserve caller blank-node identifiers. Query results are
eagerly materialized. No performance or complete-conformance claim is made.

## Inference and SHACL

```python
from owlrl_turbo import expand
from pyshacl_turbo import validate

expanded = expand(data, inference="rdfs")
# pyshacl-turbo currently requires this explicit copying conversion.
conforms, report, text = validate(expanded.to_rdflib(), shacl_graph="shapes.ttl")
```

owlrl-turbo shares this native dataset runtime. pyshacl-turbo integration currently
uses RDFLib conversion; no shared native SHACL handle is claimed. See
[the runtime ADR](docs/adr/0001-shared-native-runtime.md) and
[source provenance](vendor/README.md).

Requires Python 3.11+. Source builds require Rust 1.98.0. Both project code and
extracted inference code are MIT OR Apache-2.0; vendored sources retain licenses.
Copyright 2026 DataRoad Inc, Delaware, USA, trading as Legra.

## Development

Use `make dev` and `make check`. The initial alpha is published only by the
verified GitHub Actions workflow; see [release instructions](docs/releasing.md).

```sh
uv venv
uv pip install maturin pytest ruff
uv run maturin develop --release
uv run pytest
cargo fmt --check
cargo clippy -- -D warnings
```

CI requires 100% measured Python package line and branch coverage. This does
not measure Rust coverage, full API compatibility or standards conformance.
