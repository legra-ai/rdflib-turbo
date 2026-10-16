"""Native RDF datasets. This alpha exposes a subset of the RDFLib API."""

import json
from pathlib import Path

from rdflib import BNode, Graph, Literal, URIRef, Variable
from rdflib import Dataset as RDFLibDataset
from rdflib.query import Result

from ._native import NativeDataset

__version__ = "0.1.0a1"
__all__ = ["Dataset"]


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


class Dataset:
    """An exact-term native dataset with explicit RDFLib conversion.

    Graph mutation is available through parse(). Inference uses staged commits.
    Queries use the default graph unless default_union=True is explicitly passed.
    """

    def __init__(self):
        self._native = NativeDataset()
        self._namespaces = ()

    def __len__(self):
        return len(self._native)

    def copy(self):
        result = Dataset()
        result._native = self._native.copy()
        result._namespaces = self._namespaces
        return result

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
            dataset = Dataset()
            dataset._native.load(payload, "nquads", None, True)
            result.graph = dataset.to_rdflib().default_graph
        return result

    def infer(self, *, inference="owlrl", destination=None):
        """Expand this native dataset atomically; used by owlrl-turbo."""
        self._native.infer(inference, destination)
        return self

    @classmethod
    def from_rdflib(cls, graph):
        """Copy an RDFLib Graph or Dataset, preserving terms and blank-node IDs."""
        if not isinstance(graph, Graph):
            raise TypeError("Expected an RDFLib Graph or Dataset")
        result = cls()
        if isinstance(graph, RDFLibDataset):
            source = graph.serialize(format="nquads")
            rdf_format = "nquads"
        elif graph.context_aware:
            raise NotImplementedError("Convert context-aware graphs to rdflib.Dataset first")
        else:
            source = graph.serialize(format="nt")
            rdf_format = "nt"
        result._native.load(source, rdf_format, None, True)
        result._namespaces = tuple(graph.namespaces())
        return result

    def to_rdflib(self):
        """Copy to an RDFLib Dataset without literal normalization."""
        result = RDFLibDataset()
        for prefix, namespace in self._namespaces:
            result.bind(prefix, namespace)
        for s, p, o, g in json.loads(self._native.quads_json()):
            target = result.default_graph if g is None else result.graph(_term(g))
            target.add((_term(s), _term(p), _term(o)))
        return result
