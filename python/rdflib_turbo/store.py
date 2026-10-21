"""Transactional Oxigraph storage owned by the shared native extension."""

from contextlib import contextmanager
from os import fspath

from ._native import NativeStore
from ._operations import GraphOperations


class ValidationError(ValueError):
    """A transaction failed its SHACL commit gate; all pending changes were aborted."""

    def __init__(self, report, text):
        super().__init__(text)
        self.report = report
        self.text = text


class InferenceOperations(GraphOperations):
    """Shared inference entry point for a store and its active transaction."""

    def infer(
        self, *, inference="owlrl", destination=None, axiomatic_triples=False, datatype_axioms=False
    ):
        """Expand the graph union; write new facts to destination (default graph if omitted).

        Execution errors abort the entire transaction, including preceding writes.
        Existing statements retain their original contexts.
        """
        self._native.infer(
            inference,
            destination,
            axiomatic_triples=axiomatic_triples,
            datatype_axioms=datatype_axioms,
        )
        return self


class Transaction(InferenceOperations):
    """A scoped view of a store, including this transaction's uncommitted changes.

    The enclosing context commits on success and aborts on any exception. A failed
    native operation aborts the transaction even if its exception is caught.
    Handles cannot be reused after the context exits. No base dataset is copied.
    """

    def __init__(self, native):
        self._native = native

    @contextmanager
    def _validate_shared_shapes(self, program, settings, inplace):
        try:
            prepared = program.prepare_transaction(self._native, settings, inplace)
            yield prepared.output()
            prepared.accept()
        except BaseException:
            self._native.rollback()
            raise

    def update(self, source):
        """Execute SPARQL Update inside the current transaction."""
        self._native.update(source)
        return self


class Store(InferenceOperations):
    """An in-memory Oxigraph store, or a persistent RocksDB store at path.

    This storage API supports queries, atomic mutations and direct inference.
    pyshacl-turbo validates this store or its open transaction without copying
    the base dataset. Optional transaction shapes gate the atomic commit.
    """

    def __init__(self, path=None):
        self._native = NativeStore(None if path is None else fspath(path))

    @contextmanager
    def _validate_shared_shapes(self, program, settings, inplace):
        if inplace:
            with self.transaction() as transaction:
                with transaction._validate_shared_shapes(program, settings, True) as result:
                    yield result
        else:
            yield program.validate_store(self._native, settings, False)

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
    def transaction(self, *, shapes=None, **validation_options):
        """Commit on normal scope exit; abort on any exception or native failure.

        Queries on the transaction see its own writes. Queries on the store see
        committed state. A second writer on this handle fails immediately.
        Oxigraph buffers the transaction delta in memory, not the base dataset.
        With compiled shapes, validate after the body and before commit. Rule
        additions join the transaction; nonconformance raises ValidationError
        carrying the report. Validation options are passed to shapes.validate.
        """
        if shapes is None and validation_options:
            raise TypeError("validation options require compiled shapes")
        if "inplace" in validation_options:
            raise TypeError("the transaction gate controls inplace=True")
        if shapes is not None and not callable(getattr(shapes, "validate", None)):
            raise TypeError("shapes must provide compiled validation")
        native = self._native.transaction()
        transaction = Transaction(native)
        try:
            yield transaction
            if shapes is not None:
                conforms, report, text = shapes.validate(
                    transaction, inplace=True, **validation_options
                )
                if not conforms:
                    raise ValidationError(report, text)
        except BaseException:
            native.rollback()
            raise
        else:
            native.commit()
