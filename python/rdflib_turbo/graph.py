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
from rdflib.namespace import NamespaceManager
from rdflib.parser import create_input_source
from rdflib.query import Result
from rdflib.store import Store as ReferenceStore
from rdflib.util import from_n3, guess_format

from ._inference import GraphInference, _change_rows
from ._namespaces import NamespaceBindings
from ._native import NativeStore
from ._operations import _encode, _term
from ._ownership import GraphOwnership
from ._parsing import JsonLdOptions
from ._reads import (
    _INPUT_TERMS,
    _TERM_FACTORIES,
    ProjectedReads,
    context_names,
    encode,
    encode_graph,
    physical_rows,
    union_rows,
)
from ._serialization import GraphSerialization
from ._validation import GraphValidation
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
        if isinstance(self.backend, Store):
            for triple, cursor in union_rows(self, triple_pattern, context):
                yield (
                    triple,
                    (
                        Graph(store=self, identifier=identifier, bind_namespaces="none")
                        for identifier in context_names(self, cursor)
                    ),
                )
            return
        rows = json.loads(
            self.backend._native.match_context_quads(self._pattern(triple_pattern, context))
        )
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

    def _query_source(self, source, initNs, kwargs):
        if not isinstance(source, str):
            raise TypeError("Native SPARQL currently requires a query string")
        if kwargs:
            raise TypeError(f"Unsupported native query options: {sorted(kwargs)}")
        return (
            "\n".join(f"PREFIX {prefix}: <{uri}>" for prefix, uri in initNs.items()) + "\n" + source
        )

    def query(self, query, initNs, initBindings, queryGraph, *, _dataset_context=False, **kwargs):
        try:
            prepared = False
            if type(query) is not str:
                from rdflib.plugins.sparql.sparql import Query

                prepared = isinstance(query, Query)
            if prepared:
                from ._prepared import snapshot

                if kwargs:
                    raise TypeError(f"Unsupported native query options: {sorted(kwargs)}")
                source = snapshot(query, dataset_context=_dataset_context)
            else:
                source = self._query_source(query, initNs, kwargs)
            bindings = json.dumps(
                {str(Variable(name)): _encode(term) for name, term in initBindings.items()}
            )
        except BaseException:
            if isinstance(self.backend, Transaction):
                self.backend._native.rollback()
            raise
        if prepared:
            union = queryGraph == "__UNION__"
            graph = None if union else "" if queryGraph is None else self._graph(queryGraph)
            kind, payload = self.backend._native.query_prepared(source, graph, union, bindings)
        elif queryGraph == "__UNION__":
            kind, payload = self.backend._native.query(source, True, bindings)
        else:
            graph = "" if queryGraph is None else self._graph(queryGraph)
            kind, payload = self.backend._native.query_graph(source, graph, bindings)
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

    def update(self, update, initNs, initBindings, queryGraph, *, _dataset_context=False, **kwargs):
        try:
            source = self._query_source(update, initNs, kwargs)
            bindings = json.dumps(
                {str(Variable(name)): _encode(term) for name, term in initBindings.items()}
            )
            graph = (
                None if _dataset_context or queryGraph == "__UNION__" else self._graph(queryGraph)
            )
        except BaseException:
            if isinstance(self.backend, Transaction):
                self.backend._native.rollback()
            raise
        self.backend._native.update(source, bindings, graph)

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


class Graph(
    GraphOwnership, GraphInference, GraphValidation, GraphSerialization, ProjectedReads, RDFLibGraph
):
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
        if isinstance(store, str) and store in {"default", "Memory"}:
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
        try:
            if processor != "sparql" or not use_store_provided:
                raise ValueError("Graph only supports its native SPARQL processor")
            return self.store.update(
                update_object,
                dict(self.namespaces()) if initNs is None else initNs,
                {} if initBindings is None else initBindings,
                self.identifier,
                **kwargs,
            )
        except BaseException:
            if isinstance(self.store.backend, Transaction):
                self.store.backend._native.rollback()
            raise

    def parse(
        self, source=None, publicID=None, format=None, location=None, file=None, data=None, **args
    ):
        if isinstance(self, Graph) and (type(self) is not Graph or "add" in self.__dict__):
            # A caller-defined sink owns insertion callbacks and their ordering.
            # Parsing through a new base Graph would bypass that actual owner.
            try:
                return RDFLibGraph.parse(
                    self, source, publicID, format, location, file, data, **args
                )
            except BaseException:
                if isinstance(self.store.backend, Transaction):
                    self.store.backend._native.rollback()
                raise
        if isinstance(self.store.backend, Transaction):
            try:
                input_source = create_input_source(
                    source=source,
                    publicID=publicID,
                    location=location,
                    file=file,
                    data=data,
                    format=format,
                )
                try:
                    rdf_format = (
                        format
                        or input_source.content_type
                        or guess_format(input_source.getPublicId())
                        or "turtle"
                    )
                    options = JsonLdOptions(
                        rdf_format, args, publicID or input_source.getPublicId() or self.base
                    )
                    # RDFLib's Turtle/TriG parser accesses the manager while
                    # resolving its base, even when the document has no prefixes.
                    if options.format in {"turtle", "ttl", "trig"}:
                        _ = self.namespace_manager
                    if options.parse_plugin(input_source, self):
                        return self
                    stream = input_source.getCharacterStream() or input_source.getByteStream()
                    document = stream.read()
                    if isinstance(document, bytes):
                        document = document.decode("utf-8")
                    prefixes = json.loads(
                        self.store.backend._native.load_graph(
                            document,
                            options.format,
                            options.base,
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
                # Dataset parsing uses its default Graph's policy, independently
                # of a manager assigned to the Dataset itself. Keep initialization
                # lazy in the stage so line formats do not publish extra bindings.
                target = self.default_graph if isinstance(self, StoreDataset) else self
                if target.__dict__["_Graph__namespace_manager"] is None:
                    graph.namespace_manager = None
                    graph._bind_namespaces = target._bind_namespaces
                graph.parse(source, publicID, format, location, file, data, **args)
        return self

    @contextmanager
    def transaction(self):
        if isinstance(self.store.backend, Transaction):
            raise RuntimeError("Nested transactions are unsupported")
        owner = self.store._namespaces
        revision = owner.revision
        namespaces = owner.copy()
        reserved = False
        try:
            with self.store.backend.transaction() as transaction:
                store = RDFLibStore(
                    transaction,
                    default_identifier=self.store.default_identifier,
                    namespaces=namespaces,
                )
                yield Graph(
                    store=store, identifier=self.identifier, base=self.base, bind_namespaces="none"
                )
                prepared = owner.prepare_replacement(store._namespaces)
                owner.reserve_replacement(prepared, revision)
                reserved = True
            owner.publish_replacement(prepared)
            reserved = False
        finally:
            if reserved:
                owner.cancel_replacement(prepared)


class StoreDataset(GraphOwnership, GraphInference, GraphValidation, RDFLibDataset):
    """RDFLib Dataset over the transactional native store."""

    _turbo_native_graph = True

    def __init__(self, store="default", default_union=False, default_graph_base=None):
        if isinstance(store, str) and store in {"default", "Memory"}:
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

    @classmethod
    def _from_store(cls, store, *, default_union=False, default_graph_base=None):
        """Wrap an existing native owner without injecting new namespace bindings."""
        result = cls(
            store=store, default_union=default_union, default_graph_base=default_graph_base
        )
        manager = NamespaceManager(result, bind_namespaces="none")
        result.namespace_manager = manager
        result.default_graph.namespace_manager = manager
        return result

    @classmethod
    def from_rdflib(cls, graph):
        """Explicitly copy reference RDF terms and graph domains into one native Store."""
        if getattr(graph, "_turbo_native_graph", False):
            raise NotImplementedError("Use copy() for native Graph or Dataset inputs")
        if not isinstance(graph, RDFLibGraph):
            raise TypeError("Expected an RDFLib Graph or Dataset")
        dataset = isinstance(graph, RDFLibDataset)
        if dataset:
            default = graph.default_graph.identifier
            records = (
                (subject, predicate, obj, None if context == default else context)
                for subject, predicate, obj, context in graph.quads((None,) * 4)
            )
            declared_graphs = (
                context.identifier for context in graph.graphs() if context.identifier != default
            )
        elif graph.context_aware:
            raise NotImplementedError("Convert context-aware graphs to rdflib.Dataset first")
        else:
            records = ((subject, predicate, obj, None) for subject, predicate, obj in graph)
            declared_graphs = ()
        native = NativeStore.from_records(records, declared_graphs, _INPUT_TERMS)
        store = RDFLibStore(Store._from_native(native), namespaces=graph.namespaces())
        return cls._from_store(
            store,
            default_union=graph.default_union if dataset else False,
            default_graph_base=graph.default_graph.base if dataset else graph.base,
        )

    def infer_delta(
        self,
        *,
        inference="owlrl",
        destination=None,
        axiomatic_triples=False,
        datatype_axioms=False,
        prior_error_messages=None,
    ):
        """Infer atomically and explicitly materialize only newly added quads."""
        backend = self.store.backend
        if isinstance(backend, Transaction):
            raise NotImplementedError("Delta capture requires a committed memory Store")
        source, target, identifier = self._inference_scope(destination)
        namespaces = self.store._namespaces
        revision = namespaces.revision
        staged = namespaces.copy()
        backend._last_inference = None
        changes = backend._native._prepare_inference_changes(
            inference,
            (source, target),
            _TERM_FACTORIES,
            options=(axiomatic_triples, datatype_axioms),
            prior_error_messages=prior_error_messages,
        )
        reserved = False
        try:
            outcome = changes.outcome
            if outcome.error_messages:
                metadata_view = RDFLibStore(backend)
                metadata_view._namespaces = staged
                RDFLibGraph(
                    store=metadata_view, identifier=identifier, bind_namespaces="none"
                ).bind("err", "http://www.daml.org/2002/03/agents/agent-ont#")
            # An independent indexed result is deliberately requested by this API.
            # The private OWL path consumes committed fresh-import leases instead.
            native = NativeStore.from_records(
                _change_rows(changes.next_additions), (), _INPUT_TERMS
            )
            result = type(self)._from_store(
                RDFLibStore(Store._from_native(native), namespaces=staged.items()),
                default_union=self.default_union,
                default_graph_base=self.default_graph.base,
            )
            # The caller's pending writer applies cleanup, but removals are not
            # public output: consume their identities without restoring RDF terms.
            changes.discard_removals()
            prepared = namespaces.prepare_replacement(staged)
            namespaces.reserve_replacement(prepared, revision)
            reserved = True
            changes.accept()
            namespaces.publish_replacement(prepared)
            reserved = False
            backend._last_inference = outcome
            result.store.backend._last_inference = outcome
            return result
        finally:
            if reserved:
                namespaces.cancel_replacement(prepared)
            changes.close()

    def to_rdflib(self):
        """Explicitly copy complete lexical terms and empty graph domains to RDFLib."""
        result = RDFLibDataset(
            default_union=self.default_union, default_graph_base=self.default_graph.base
        )
        manager = NamespaceManager(result, bind_namespaces="none")
        result.namespace_manager = manager
        result.default_graph.namespace_manager = manager
        for prefix, namespace in self.namespaces():
            result.bind(prefix, namespace, replace=True)
        for context in self.graphs():
            target = (
                result.default_graph
                if context.identifier == self.default_graph.identifier
                else result.graph(context.identifier)
            )
            # Each native scoped iterator owns its bounded read batches. Never
            # hydrate the dataset-wide quad grouping just to copy its output.
            for triple in context:
                target.add(triple)
        return result

    def graph(self, identifier=None, base=None):
        return super().graph(identifier, base)

    def quads(self, quad=None):
        if isinstance(self.store.backend, Transaction):
            # The existing transaction reader includes its uncommitted writes.
            # Committed snapshot cursors must never replace that owner.
            yield from super().quads(quad)
            return
        subject, predicate, obj, context = self._spoc(quad)
        if context is None:
            yield from physical_rows(self.store, (subject, predicate, obj), context)
        else:
            for triple, cursor in union_rows(self.store, (subject, predicate, obj), context):
                for identifier in context_names(self.store, cursor):
                    yield (*triple, identifier)

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
            _dataset_context=True,
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
        try:
            if processor != "sparql" or not use_store_provided:
                raise ValueError("StoreDataset only supports its native SPARQL processor")
            return self.store.update(
                update_object,
                dict(self.namespaces()) if initNs is None else initNs,
                {} if initBindings is None else initBindings,
                DATASET_DEFAULT_GRAPH_ID,
                _dataset_context=True,
                **kwargs,
            )
        except BaseException:
            if isinstance(self.store.backend, Transaction):
                self.store.backend._native.rollback()
            raise

    parse = Graph.parse

    @contextmanager
    def transaction(self):
        if isinstance(self.store.backend, Transaction):
            raise RuntimeError("Nested transactions are unsupported")
        owner = self.store._namespaces
        revision = owner.revision
        namespaces = owner.copy()
        reserved = False
        try:
            with self.store.backend.transaction() as transaction:
                store = RDFLibStore(transaction, namespaces=namespaces)
                yield StoreDataset._from_store(
                    store,
                    default_union=self.default_union,
                    default_graph_base=self.default_graph.base,
                )
                prepared = owner.prepare_replacement(store._namespaces)
                owner.reserve_replacement(prepared, revision)
                reserved = True
            owner.publish_replacement(prepared)
            reserved = False
        finally:
            if reserved:
                owner.cancel_replacement(prepared)
