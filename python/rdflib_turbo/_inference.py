"""Inference borrows the caller's scoped Store or active transaction."""

from rdflib import BNode, URIRef
from rdflib import Dataset as RDFLibDataset
from rdflib import Graph as RDFLibGraph

from ._reads import _TERM_FACTORIES
from .store import Transaction

_ERROR_NAMESPACE = "http://www.daml.org/2002/03/agents/agent-ont#"


def _change_rows(next_batch):
    # bounded: one native batch (128 rows), plus the consumer's bounded import pins.
    while True:
        rows, done = next_batch()
        yield from rows
        if done:
            return


class GraphInference:
    def infer(
        self,
        *,
        inference="owlrl",
        destination=None,
        axiomatic_triples=False,
        datatype_axioms=False,
        prior_error_messages=None,
    ):
        """Infer in place, staging diagnostic namespaces with the RDF writes.

        A Graph reads its scope; a Dataset reads its union. A separate destination
        receives consequences without becoming a source for later rounds.
        """
        backend = self.store.backend
        backend._last_inference = None
        try:
            scope = self._inference_scope(destination)
            options = {
                "axiomatic_triples": axiomatic_triples,
                "datatype_axioms": datatype_axioms,
                "prior_error_messages": prior_error_messages,
            }
            if isinstance(backend, Transaction):
                outcome = self._infer_bound_scope(inference, *scope, **options)
            else:
                # Namespace metadata alone is copied by the existing transaction.
                # Native inference continues to borrow the same RDF store.
                with self.transaction() as transaction:
                    outcome = transaction._infer_bound_scope(inference, *scope, **options)
            backend._last_inference = outcome
        except BaseException:
            if isinstance(backend, Transaction):
                backend._native.rollback()
            raise
        return self

    def _capture_changes(
        self,
        *,
        inference="owlrl",
        destination=None,
        axiomatic_triples=False,
        datatype_axioms=False,
        prior_error_messages=None,
    ):
        """Commit a private imported owner; close both leases in finally.

        Ordinary OWL writeback uses this only after constructing a fresh,
        unpublished import. Public delta results require prepared acceptance.
        """
        backend = self.store.backend
        if isinstance(backend, Transaction):
            raise NotImplementedError("Delta capture requires a committed memory Store")
        source, target, identifier = self._inference_scope(destination)
        backend._last_inference = None
        changes = backend._native._infer_changes(
            inference,
            (source, target),
            _TERM_FACTORIES,
            options=(axiomatic_triples, datatype_axioms),
            prior_error_messages=prior_error_messages,
        )
        try:
            outcome = changes.outcome
            if outcome.error_messages:
                destination = RDFLibGraph(
                    store=self.store, identifier=identifier, bind_namespaces="none"
                )
                destination.bind("err", _ERROR_NAMESPACE)
            backend._last_inference = outcome
            return changes
        except BaseException:
            changes.close()
            raise

    def _infer_bound_scope(
        self, inference, source, destination_graph, destination_identifier, **options
    ):
        outcome = self.store.backend._native.infer_graph(
            inference, (source, destination_graph), **options
        )
        if outcome.error_messages:
            # A metadata-only view preserves the destination identity and uses
            # the staged native store's existing namespace mapping.
            destination = RDFLibGraph(
                store=self.store, identifier=destination_identifier, bind_namespaces="none"
            )
            destination.bind("err", _ERROR_NAMESPACE)
        return outcome

    def _inference_scope(self, destination):
        source = None if isinstance(self, RDFLibDataset) else self.store._graph(self)
        if destination is None:
            target = self.default_graph if isinstance(self, RDFLibDataset) else self
            destination_graph = self.store._graph(target)
            identifier = target.identifier
        elif isinstance(destination, RDFLibGraph):
            if getattr(destination.store, "backend", None) is not self.store.backend:
                raise ValueError("Source and destination must share the same native store")
            destination_graph = destination.store._graph(destination)
            identifier = destination.identifier
        elif isinstance(destination, (URIRef, BNode)) or type(destination) is str:
            identifier = destination if isinstance(destination, BNode) else URIRef(destination)
            destination_graph = self.store._graph(identifier)
        else:
            raise TypeError("Expected a native Graph or graph identifier destination")
        return source, destination_graph, identifier
