"""RDFLib's Python facade over one exact-term native Store or transaction.

RDFLib supplies terms, namespace management and parser/serializer boundaries.
Its in-memory graph stores and Python SPARQL evaluators never execute here.
"""

import json
from contextlib import contextmanager

from rdflib import BNode, URIRef, Variable
from rdflib import Dataset as RDFLibDataset
from rdflib import Graph as RDFLibGraph
from rdflib.graph import DATASET_DEFAULT_GRAPH_ID
from rdflib.parser import create_input_source
from rdflib.query import Result
from rdflib.store import Store as ReferenceStore
from rdflib.util import from_n3, guess_format

from ._inference import GraphInference
from ._namespaces import NamespaceBindings
from ._operations import _encode, _term
from ._ownership import GraphOwnership
from ._reads import ProjectedReads, encode, encode_graph, scoped_rows
from .store import Store, Transaction


class RDFLibStore(ReferenceStore):
    """Context-aware RDFLib Store contract backed exclusively by native storage."""

    context_aware = True
    graph_aware = True
    transaction_aware = True
    formula_aware = False

    def __init__(
        self, backend=None, *, default_identifier=DATASET_DEFAULT_GRAPH_ID, namespaces=None
    ):
        super().__init__()
        if backend is not None and not isinstance(backend, (Store, Transaction)):
            raise TypeError("Native Store or Transaction required")
        self.backend = Store() if backend is None else backend
        self.default_identifier = default_identifier
        if namespaces is None:
            if not hasattr(self.backend, "_rdflib_namespaces"):
                self.backend._rdflib_namespaces = NamespaceBindings()
            namespaces = self.backend._rdflib_namespaces
        else:
            namespaces = NamespaceBindings(namespaces)
            self.backend._rdflib_namespaces = namespaces
        self._namespaces = namespaces

    def _graph(self, context):
        if context is None:
            return None
        identifier = context.identifier if hasattr(context, "identifier") else context
        return "" if identifier == self.default_identifier else json.dumps(_encode(identifier))

    def _pattern(self, triple, context):
        return json.dumps([*map(_encode, triple), self._graph(context)])

    def add(self, triple, context, quoted=False):
        if quoted:
            raise ValueError("Quoted graphs are unsupported")
        if context is None:
            raise ValueError("Graph context is required")
        self.backend._native.insert_terms(*map(encode, triple), encode_graph(self, context))

    def remove(self, triple, context=None):
        self.backend._native.remove_pattern(self._pattern(triple, context))

    def triples(self, triple_pattern, context=None):
        if context is not None and isinstance(self.backend, Store):
            for triple in scoped_rows(self, triple_pattern, context):
                yield triple, iter((context,))
            return
        rows = json.loads(self.backend._native.match_quads(self._pattern(triple_pattern, context)))
        grouped = {}
        for s, p, o, g in rows:
            triple = (_term(s), _term(p), _term(o))
            identifier = self.default_identifier if g is None else _term(g)
            grouped.setdefault(triple, []).append(identifier)
        for triple, contexts in grouped.items():
            yield (
                triple,
                (
                    Graph(store=self, identifier=identifier, bind_namespaces="none")
                    for identifier in contexts
                ),
            )

    def __len__(self, context=None):
        if context is None:
            return self.backend._native.count_pattern("[null,null,null,null]")
        return self.backend._native.graph_len(encode_graph(self, context))

    def contexts(self, triple=None):
        if triple is not None:
            identifiers = {context for _, contexts in self.triples(triple) for context in contexts}
        else:
            identifiers = {
                from_n3(value) for value in json.loads(self.backend._native.named_graphs())
            }
            if len(self.default_graph()):
                identifiers.add(self.default_identifier)
        yield from identifiers

    def default_graph(self):
        return Graph(store=self, identifier=self.default_identifier)

    def add_graph(self, graph):
        if (
            graph.identifier != self.default_identifier
            and not self.backend._native.contains_named_graph(encode(graph.identifier))
        ):
            self.backend._native.change_graph(self._graph(graph), False)

    def remove_graph(self, graph):
        name = self._graph(graph)
        if name:
            self.backend._native.change_graph(name, True)
        else:
            self.remove((None, None, None), graph)

    def bind(self, prefix, namespace, override=True):
        self._namespaces.bind(prefix, namespace, override)

    def prefix(self, namespace):
        return self._namespaces.prefix(namespace)

    def namespace(self, prefix):
        return self._namespaces.namespace(prefix)

    def namespaces(self):
        yield from self._namespaces.items()

    def _query_source(self, source, initNs, initBindings, kwargs):
        if not isinstance(source, str):
            raise TypeError("Native SPARQL currently requires a query string")
        if initBindings:
            raise NotImplementedError("Native initial query bindings are not implemented")
        if kwargs:
            raise TypeError(f"Unsupported native query options: {sorted(kwargs)}")
        return (
            "\n".join(f"PREFIX {prefix}: <{uri}>" for prefix, uri in initNs.items()) + "\n" + source
        )

    def query(self, query, initNs, initBindings, queryGraph, **kwargs):
        source = self._query_source(query, initNs, initBindings, kwargs)
        if queryGraph == "__UNION__":
            kind, payload = self.backend._native.query(source, True)
        else:
            graph = "" if queryGraph is None else self._graph(queryGraph)
            kind, payload = self.backend._native.query_graph(source, graph)
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
            # A requested graph result is newly owned output, never a copy of input storage.
            result.graph = Graph().parse(data=payload, format="nt")
        return result

    def update(self, update, initNs, initBindings, queryGraph, **kwargs):
        source = self._query_source(update, initNs, initBindings, kwargs)
        if queryGraph not in (None, "__UNION__", self.default_identifier):
            raise NotImplementedError("Scoped named-graph SPARQL Update is not implemented")
        self.backend.update(source)

    def commit(self):
        raise RuntimeError("Use Graph.transaction() or StoreDataset.transaction() scoped context")

    def rollback(self):
        raise RuntimeError("Use Graph.transaction() or StoreDataset.transaction() scoped context")

    def close(self, commit_pending_transaction=False):
        if commit_pending_transaction:
            raise ValueError("Close cannot commit a scoped transaction")
        if isinstance(self.backend, Transaction):
            raise RuntimeError("Close the enclosing transaction context")
        self.backend.close()


class Graph(GraphOwnership, GraphInference, ProjectedReads, RDFLibGraph):
    """RDFLib Graph facade with exact-term native storage and native SPARQL."""

    _turbo_native_graph = True

    def __init__(
        self,
        store="default",
        identifier=None,
        namespace_manager=None,
        base=None,
        bind_namespaces="rdflib",
    ):
        identifier = (
            BNode()
            if identifier is None
            else identifier
            if isinstance(identifier, (URIRef, BNode))
            else URIRef(identifier)
        )
        if store == "default":
            store = RDFLibStore(default_identifier=identifier)
        elif isinstance(store, (Store, Transaction)):
            store = RDFLibStore(store)
        elif not isinstance(store, RDFLibStore):
            raise TypeError("Graph requires a native RDFLibStore, Store or Transaction")
        super().__init__(
            store=store,
            identifier=identifier,
            namespace_manager=namespace_manager,
            base=base,
            bind_namespaces=bind_namespaces,
        )

    def query(
        self,
        query_object,
        processor="sparql",
        result="sparql",
        initNs=None,
        initBindings=None,
        use_store_provided=True,
        **kwargs,
    ):
        if processor != "sparql" or result != "sparql" or not use_store_provided:
            raise ValueError("Graph only supports its native SPARQL processor")
        return self.store.query(
            query_object,
            dict(self.namespaces()) if initNs is None else initNs,
            {} if initBindings is None else initBindings,
            self.identifier,
            **kwargs,
        )

    def update(
        self,
        update_object,
        processor="sparql",
        initNs=None,
        initBindings=None,
        use_store_provided=True,
        **kwargs,
    ):
        if processor != "sparql" or not use_store_provided:
            raise ValueError("Graph only supports its native SPARQL processor")
        return self.store.update(
            update_object,
            dict(self.namespaces()) if initNs is None else initNs,
            {} if initBindings is None else initBindings,
            self.identifier,
            **kwargs,
        )

    def parse(
        self, source=None, publicID=None, format=None, location=None, file=None, data=None, **args
    ):
        if args:
            raise TypeError(f"Unsupported native parser options: {sorted(args)}")
        if isinstance(self.store.backend, Transaction):
            try:
                input_source = create_input_source(
                    source=source, publicID=publicID, location=location, file=file, data=data
                )
                try:
                    stream = input_source.getCharacterStream() or input_source.getByteStream()
                    document = stream.read()
                    if isinstance(document, bytes):
                        document = document.decode("utf-8")
                    rdf_format = (
                        format
                        or input_source.content_type
                        or guess_format(input_source.getPublicId())
                        or "turtle"
                    )
                    prefixes = json.loads(
                        self.store.backend._native.load_graph(
                            document,
                            rdf_format,
                            publicID or input_source.getPublicId() or self.base,
                            "" if isinstance(self, StoreDataset) else self.store._graph(self),
                            isinstance(self, StoreDataset),
                        )
                    )
                    for prefix, namespace in prefixes:
                        self.bind(prefix, URIRef(namespace))
                finally:
                    if input_source.auto_close:
                        input_source.close()
            except BaseException:
                self.store.backend._native.rollback()
                raise
        else:
            with self.transaction() as graph:
                graph.parse(source, publicID, format, location, file, data)
        return self

    @contextmanager
    def transaction(self):
        if isinstance(self.store.backend, Transaction):
            raise RuntimeError("Nested transactions are unsupported")
        namespaces = self.store._namespaces.copy()
        with self.store.backend.transaction() as transaction:
            store = RDFLibStore(
                transaction, default_identifier=self.store.default_identifier, namespaces=namespaces
            )
            yield Graph(
                store=store, identifier=self.identifier, base=self.base, bind_namespaces="none"
            )
        self.store._namespaces.replace(store._namespaces)


class StoreDataset(GraphOwnership, GraphInference, RDFLibDataset):
    """RDFLib Dataset over the transactional native store."""

    _turbo_native_graph = True

    def __init__(self, store="default", default_union=False, default_graph_base=None):
        if store == "default":
            store = RDFLibStore()
        elif isinstance(store, (Store, Transaction)):
            store = RDFLibStore(store)
        elif not isinstance(store, RDFLibStore):
            raise TypeError("StoreDataset requires native storage")
        super().__init__(
            store=store, default_union=default_union, default_graph_base=default_graph_base
        )
        self.default_graph = Graph(
            store=self.store, identifier=DATASET_DEFAULT_GRAPH_ID, base=default_graph_base
        )

    def graph(self, identifier=None, base=None):
        graph = Graph(self.store, identifier, self.namespace_manager, base)
        self.store.add_graph(graph)
        return graph

    def get_context(self, identifier, quoted=False, base=None):
        if quoted:
            raise ValueError("Quoted graphs are unsupported")
        return Graph(self.store, identifier, self.namespace_manager, base)

    def query(
        self,
        query_object,
        processor="sparql",
        result="sparql",
        initNs=None,
        initBindings=None,
        use_store_provided=True,
        **kwargs,
    ):
        if processor != "sparql" or result != "sparql" or not use_store_provided:
            raise ValueError("StoreDataset only supports its native SPARQL processor")
        return self.store.query(
            query_object,
            dict(self.namespaces()) if initNs is None else initNs,
            {} if initBindings is None else initBindings,
            "__UNION__" if self.default_union else DATASET_DEFAULT_GRAPH_ID,
            **kwargs,
        )

    def update(
        self,
        update_object,
        processor="sparql",
        initNs=None,
        initBindings=None,
        use_store_provided=True,
        **kwargs,
    ):
        if processor != "sparql" or not use_store_provided:
            raise ValueError("StoreDataset only supports its native SPARQL processor")
        return self.store.update(
            update_object,
            dict(self.namespaces()) if initNs is None else initNs,
            {} if initBindings is None else initBindings,
            DATASET_DEFAULT_GRAPH_ID,
            **kwargs,
        )

    parse = Graph.parse

    @contextmanager
    def transaction(self):
        if isinstance(self.store.backend, Transaction):
            raise RuntimeError("Nested transactions are unsupported")
        namespaces = self.store._namespaces.copy()
        with self.store.backend.transaction() as transaction:
            store = RDFLibStore(transaction, namespaces=namespaces)
            yield StoreDataset(store=store, default_union=self.default_union)
        self.store._namespaces.replace(store._namespaces)
