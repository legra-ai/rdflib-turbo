"""Inference borrows the caller's scoped Store or active transaction."""

from rdflib import BNode, URIRef
from rdflib import Dataset as RDFLibDataset
from rdflib import Graph as RDFLibGraph

from .store import Transaction


class GraphInference:
    def infer(self, *, inference="owlrl", destination=None):
        """Infer in place, reading this graph or a Dataset's union.

        A separate destination receives consequences without becoming a source.
        All writes belong to the existing native transaction boundary.
        """
        backend = self.store.backend
        try:
            source = None if isinstance(self, RDFLibDataset) else self.store._graph(self)
            if destination is None:
                target = self.default_graph if isinstance(self, RDFLibDataset) else self
                destination_graph = self.store._graph(target)
            elif isinstance(destination, RDFLibGraph):
                if getattr(destination.store, "backend", None) is not backend:
                    raise ValueError("Source and destination must share the same native store")
                destination_graph = destination.store._graph(destination)
            elif isinstance(destination, (URIRef, BNode)) or type(destination) is str:
                identifier = destination if isinstance(destination, BNode) else URIRef(destination)
                destination_graph = self.store._graph(identifier)
            else:
                raise TypeError("Expected a native Graph or graph identifier destination")
            backend._native.infer_graph(inference, source, destination_graph)
        except BaseException:
            if isinstance(backend, Transaction):
                backend._native.rollback()
            raise
        return self
