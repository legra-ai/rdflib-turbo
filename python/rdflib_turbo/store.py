"""Transactional Oxigraph storage owned by the shared native extension."""

from contextlib import contextmanager
from os import fspath

from ._native import NativeStore
from ._operations import GraphOperations


class InferenceOperations(GraphOperations):
    """Shared inference entry point for a store and its active transaction."""

    def infer(self, *, inference="owlrl", destination=None):
        """Expand the graph union; write new facts to destination (default graph if omitted).

        Execution errors abort the entire transaction, including preceding writes.
        Existing statements retain their original contexts.
        """
        self._native.infer(inference, destination)
        return self


class Transaction(InferenceOperations):
    """A scoped view of a store, including this transaction's uncommitted changes.

    The enclosing context commits on success and aborts on any exception. A failed
    native operation aborts the transaction even if its exception is caught.
    Handles cannot be reused after the context exits. No base dataset is copied.
    """

    def __init__(self, native):
        self._native = native

    def update(self, source):
        """Execute SPARQL Update inside the current transaction."""
        self._native.update(source)
        return self


class Store(InferenceOperations):
    """An in-memory Oxigraph store, or a persistent RocksDB store at path.

    This storage API supports queries, atomic mutations and direct inference.
    SHACL remains separate until its direct store adapter lands.
    """

    def __init__(self, path=None):
        self._native = NativeStore(None if path is None else fspath(path))

    def close(self):
        """Release the store; an active transaction must finish first."""
        self._native.close()

    def __enter__(self):
        self._native.ensure_open()
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        self.close()

    def update(self, source):
        """Commit a SPARQL Update atomically, or abort it on error."""
        self._native.update(source)
        return self

    @contextmanager
    def transaction(self):
        """Commit on normal scope exit; abort on any exception or native failure.

        Queries on the transaction see its own writes. Queries on the store see
        committed state. A second writer on this handle fails immediately.
        Oxigraph buffers the transaction delta in memory, not the base dataset.
        """
        native = self._native.transaction()
        try:
            yield Transaction(native)
        except BaseException:
            native.rollback()
            raise
        else:
            native.commit()
