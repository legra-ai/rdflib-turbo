"""Shared Python input and result conversion for the native backends."""

import json
from os import PathLike
from pathlib import Path
from typing import Self

from rdflib import BNode, Graph, Literal, URIRef, Variable
from rdflib.query import Result

from ._literal import restore
from ._native import NativeDataset


def _encode(value):
    if value is None:
        return None
    if isinstance(value, URIRef):
        return {"type": "uri", "value": str(value)}
    if isinstance(value, BNode):
        return {"type": "bnode", "value": str(value)}
    if isinstance(value, Literal):
        result = {"type": "literal", "value": str(value)}
        if value.language:
            result["xml:lang"] = value.language
        elif value.datatype:
            result["datatype"] = str(value.datatype)
        return result
    raise TypeError(f"Unsupported RDF term: {type(value).__name__}")


def _term(value):
    if value is None:
        return None
    kind = value["type"]
    if kind == "uri":
        return str.__new__(URIRef, value["value"])
    if kind == "bnode":
        return str.__new__(BNode, value["value"])
    if kind == "literal":
        return restore(value["value"], value.get("datatype"), value.get("xml:lang"))
    raise ValueError(f"Unknown native RDF term: {kind}")


class GraphOperations:
    """RDF input, output and SPARQL query operations on a native handle."""

    def __len__(self) -> int:
        return len(self._native)

    def parse(
        self,
        source: str | PathLike[str] | None = None,
        *,
        data: str | bytes | None = None,
        format: str = "turtle",
        publicID: str | None = None,
    ) -> Self:
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

    def serialize(self, *, format: str = "nquads") -> str:
        """Serialize natively; graph-only formats reject named graph data."""
        return self._native.dump(format)

    def query(self, query: str, *, default_union: bool = False) -> Result:
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
