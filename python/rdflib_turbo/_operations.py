"""Shared Python input and result conversion for the native backends."""

import json
from pathlib import Path

from rdflib import BNode, Graph, Literal, URIRef, Variable
from rdflib.query import Result

from ._native import NativeDataset


def _term(value):
    if value is None:
        return None
    kind = value["type"]
    if kind == "uri":
        return URIRef(value["value"])
    if kind == "bnode":
        return BNode(value["value"])
    if kind == "literal":
        return Literal(
            value["value"],
            lang=value.get("xml:lang"),
            datatype=value.get("datatype"),
            normalize=False,
        )
    raise ValueError(f"Unknown native RDF term: {kind}")


class GraphOperations:
    """RDF input, output and SPARQL query operations on a native handle."""

    def __len__(self):
        return len(self._native)

    def parse(self, source=None, *, data=None, format="turtle", publicID=None):
        """Append a UTF-8 document atomically. source is a local file path."""
        if (source is None) == (data is None):
            raise TypeError("Specify exactly one of source or data")
        if source is not None:
            path = Path(source).resolve()
            data = path.read_text(encoding="utf-8")
            if publicID is None:
                publicID = path.as_uri()
        if isinstance(data, bytes):
            data = data.decode("utf-8")
        if not isinstance(data, str):
            raise TypeError("RDF data must be str or UTF-8 bytes")
        self._native.load(data, format, publicID)
        return self

    def serialize(self, *, format="nquads"):
        """Serialize natively; graph-only formats reject named graph data."""
        return self._native.dump(format)

    def query(self, query, *, default_union=False):
        """Execute SELECT, ASK, CONSTRUCT or DESCRIBE; return an RDFLib Result."""
        kind, payload = self._native.query(query, default_union)
        result = Result(kind)
        if kind == "ASK":
            result.askAnswer = json.loads(payload)
        elif kind == "SELECT":
            value = json.loads(payload)
            result.vars = [Variable(name) for name in value["variables"]]
            result.bindings = [
                {Variable(name): _term(term) for name, term in row.items()} for row in value["rows"]
            ]
        else:
            native = NativeDataset()
            native.load(payload, "nquads", None, True)
            result.graph = Graph()
            for subject, predicate, obj, _ in json.loads(native.quads_json()):
                result.graph.add((_term(subject), _term(predicate), _term(obj)))
        return result
