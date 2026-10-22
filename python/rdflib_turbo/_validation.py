"""Borrow native graph views through report acceptance and atomic rule writes."""

from contextlib import contextmanager

from rdflib import Dataset as RDFLibDataset

from .store import Transaction


class GraphValidation:
    @contextmanager
    def _validate_shared_shapes(self, program, settings, inplace):
        backend = self.store.backend
        namespaces = self.store._namespaces
        revision = namespaces.revision
        try:
            if isinstance(backend, Transaction):
                scope = None if isinstance(self, RDFLibDataset) else self.store._graph(self)
                prepared = program.prepare_transaction(backend._native, settings, inplace, scope)
                yield prepared.output()
                self._accept_namespaces(namespaces, revision)
                prepared.accept()
            elif inplace:
                with self.transaction() as transaction:
                    with transaction._validate_shared_shapes(program, settings, True) as result:
                        yield result
                        self._accept_namespaces(namespaces, revision)
            else:
                scope = None if isinstance(self, RDFLibDataset) else self.store._graph(self)
                yield program.validate_store(backend._native, settings, False, scope)
                self._accept_namespaces(namespaces, revision)
        except BaseException:
            if isinstance(backend, Transaction):
                backend._native.rollback()
            raise

    def _accept_namespaces(self, namespaces, revision):
        if namespaces.revision != revision:
            raise RuntimeError("Graph namespaces changed while processing validation output")
