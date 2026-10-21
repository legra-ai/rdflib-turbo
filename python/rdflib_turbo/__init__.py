"""Native RDF datasets. This alpha exposes a subset of the RDFLib API."""

import json
from collections.abc import Iterator
from contextlib import contextmanager
from importlib.metadata import version
from typing import Self

from rdflib import Dataset as RDFLibDataset
from rdflib import Graph as RDFLibGraph

from ._native import NativeDataset
from ._operations import GraphOperations, _term
from .graph import Graph, StoreDataset
from .store import Store, Transaction, ValidationError

__version__ = version("rdflib-turbo")
__all__ = ["Dataset", "Graph", "Store", "StoreDataset", "Transaction", "ValidationError"]


class Dataset(GraphOperations):
    """An exact-term native dataset with explicit RDFLib conversion.

    Graph mutation is available through parse(). Inference uses staged commits.
    Queries use the default graph unless default_union=True is explicitly passed.
    """

    def __init__(self) -> None:
        self._native = NativeDataset()

    def copy(self) -> "Dataset":
        result = Dataset()
        result._native = self._native.copy()
        return result

    @contextmanager
    def _validate_shared_shapes(self, program, settings, inplace):
        yield program.validate(self._native, settings, inplace)

    def namespaces(self) -> Iterator[tuple[str, str]]:
        """Return namespace bindings retained atomically from parsed documents."""
        return iter(self._native.namespaces())

    def infer(
        self,
        *,
        inference: str = "owlrl",
        destination: str | None = None,
        axiomatic_triples: bool = False,
        datatype_axioms: bool = False,
    ) -> Self:
        """Expand this native dataset atomically; used by owlrl-turbo."""
        self._native.infer(
            inference,
            destination,
            axiomatic_triples=axiomatic_triples,
            datatype_axioms=datatype_axioms,
        )
        return self

    def infer_delta(
        self,
        *,
        inference: str = "owlrl",
        destination: str | None = None,
        axiomatic_triples: bool = False,
        datatype_axioms: bool = False,
    ) -> "Dataset":
        """Expand atomically and return a native dataset of newly added quads.

        The result retains namespace bindings and exact output graph contexts.
        It excludes original quads; failures leave this dataset unchanged.
        """
        result = Dataset()
        result._native = self._native.infer_delta(
            inference,
            destination,
            axiomatic_triples=axiomatic_triples,
            datatype_axioms=datatype_axioms,
        )
        return result

    @classmethod
    def from_rdflib(cls, graph: RDFLibGraph) -> Self:
        """Copy an RDFLib Graph or Dataset, preserving terms and blank-node IDs."""
        if getattr(graph, "_turbo_native_graph", False):
            raise NotImplementedError("Native Store inference/SHACL adapters are not implemented")
        if not isinstance(graph, RDFLibGraph):
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
        result._native.bind_namespaces(
            [(prefix, str(namespace)) for prefix, namespace in graph.namespaces()]
        )
        return result

    def to_rdflib(self) -> RDFLibDataset:
        """Copy to an RDFLib Dataset without literal normalization."""
        result = RDFLibDataset()
        for prefix, namespace in self.namespaces():
            result.bind(prefix, namespace)
        for s, p, o, g in json.loads(self._native.quads_json()):
            target = result.default_graph if g is None else result.graph(_term(g))
            target.add((_term(s), _term(p), _term(o)))
        return result
