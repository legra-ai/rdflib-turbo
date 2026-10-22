"""Borrow native graph views through report acceptance and atomic rule writes."""

import json
from contextlib import contextmanager

from rdflib import Dataset as RDFLibDataset

from .store import Transaction

_ERROR_NAMESPACE = "http://www.daml.org/2002/03/agents/agent-ont#"


def _unique_settings(pairs):
    settings = {}
    for key, value in pairs:
        if key in settings:
            raise ValueError(f"duplicate field {key}")
        settings[key] = value
    return settings


def _bound_settings(settings, namespaces):
    value = json.loads(settings, object_pairs_hook=_unique_settings)
    if not isinstance(value, dict):
        raise ValueError("Expected a validation settings object")
    if "data_namespaces" in value:
        raise ValueError("Graph validation owns its data_namespaces settings")
    value["data_namespaces"] = list(namespaces.items())
    return json.dumps(value)


class GraphValidation:
    @contextmanager
    def _validate_shared_shapes(self, program, settings, inplace):
        backend = self.store.backend
        namespaces = self.store._namespaces
        revision = namespaces.revision
        try:
            bound_settings = _bound_settings(settings, namespaces)
            if isinstance(backend, Transaction):
                scope = None if isinstance(self, RDFLibDataset) else self.store._graph(self)
                prepared = program.prepare_transaction(
                    backend._native, bound_settings, inplace, scope
                )
                yield prepared.output()
                self._accept_namespaces(namespaces, revision)
                if inplace and prepared.has_diagnostics():
                    self.bind("err", _ERROR_NAMESPACE)
                prepared.accept()
            elif inplace:
                with self.transaction() as transaction:
                    with transaction._validate_shared_shapes(program, settings, True) as result:
                        yield result
                    self._accept_namespaces(namespaces, revision)
            else:
                scope = None if isinstance(self, RDFLibDataset) else self.store._graph(self)
                yield program.validate_store(backend._native, bound_settings, False, scope)
                self._accept_namespaces(namespaces, revision)
        except BaseException:
            if isinstance(backend, Transaction):
                backend._native.rollback()
            raise

    def _accept_namespaces(self, namespaces, revision):
        if namespaces.revision != revision:
            raise RuntimeError("Graph namespaces changed while processing validation output")
