"""Native graph ownership operations, independent from RDFLib RDF transport."""

from rdflib import Dataset as RDFLibDataset

from .store import Store


class GraphOwnership:
    @property
    def last_inference(self):
        """The owner's latest successful invocation; provisional in a transaction."""
        return self.store.backend._last_inference

    def copy(self):
        """Own an independent native copy of this scope, including pending writes."""
        from .graph import Graph, RDFLibStore, StoreDataset

        dataset = isinstance(self, RDFLibDataset)
        graph = None if dataset else self.store._graph(self)
        native = self.store.backend._native.fork_graph(graph)
        store = RDFLibStore(
            Store._from_native(native),
            default_identifier=self.store.default_identifier,
            namespaces=self.store._namespaces,
        )
        if dataset:
            return StoreDataset(
                store=store,
                default_union=self.default_union,
                default_graph_base=self.default_graph.base,
            )
        return Graph(
            store=store, identifier=self.identifier, base=self.base, bind_namespaces="none"
        )
