"""Graph-scoped native output for the ordinary N-Triples return-value path."""

from rdflib import plugin
from rdflib.plugins.serializers.nt import NT11Serializer, NTSerializer
from rdflib.serializer import Serializer

from ._reads import encode_graph


class GraphSerialization:
    def serialize(self, destination=None, format="turtle", base=None, encoding=None, **args):
        # Import after graph construction to keep its facade/mixin dependency acyclic.
        from .graph import Graph

        if (
            type(self) is Graph
            and destination is None
            and format in ("nt", "nt11", "ntriples")
            and base is None
            and self.base is None
            and encoding in (None, "utf-8")
            and not args
            and plugin.get(format, Serializer) in (NTSerializer, NT11Serializer)
        ):
            return self.store.backend._native._dump_graph_nt(
                encode_graph(self.store, self), as_bytes=encoding is not None
            )
        return super().serialize(destination, format, base, encoding, **args)
