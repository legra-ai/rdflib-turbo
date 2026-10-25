"""Store-owned namespace metadata with constant-time lookup in both directions."""

from threading import Lock

from rdflib import URIRef


class NamespaceBindings:
    def __init__(self, source=()):
        self.revision = 0
        self._guard = Lock()
        self._publication = None
        if isinstance(source, NamespaceBindings):
            self._namespaces, self._prefixes = source._snapshot_maps()
        else:
            self._namespaces = dict(source)
            self._prefixes = {
                namespace: prefix for prefix, namespace in reversed(self._namespaces.items())
            }

    def namespace(self, prefix):
        return self._namespaces.get(prefix)

    def items(self):
        return self._namespaces.items()

    def prefix(self, namespace):
        return self._prefixes.get(namespace)

    def bind(self, prefix, namespace, override=True):
        # Caller coercion can execute arbitrary Python and release the GIL. Do
        # it before acquiring the guard, then fail if publication began meanwhile.
        prefix, namespace = str(prefix), URIRef(namespace)
        self._acquire()
        try:
            self.revision += 1
            bound_namespace = self._namespaces.get(prefix)
            bound_prefix = self._prefixes.get(namespace)
            if bound_prefix is None:
                bound_prefix = self._prefixes.get(bound_namespace)
            if override:
                if bound_prefix is not None:
                    del self._namespaces[bound_prefix]
                if bound_namespace is not None:
                    del self._prefixes[bound_namespace]
                self._prefixes[namespace] = prefix
                self._namespaces[prefix] = namespace
            else:
                key = bound_namespace if bound_namespace is not None else namespace
                value = bound_prefix if bound_prefix is not None else prefix
                self._prefixes[key] = value
                self._namespaces[value] = key
        finally:
            self._guard.release()

    def copy(self):
        return type(self)(self)

    def _acquire(self):
        if not self._guard.acquire(blocking=False):
            raise RuntimeError("Namespace bindings are busy during mutation or publication")

    def _snapshot_maps(self):
        self._acquire()
        try:
            return self._namespaces.copy(), self._prefixes.copy()
        finally:
            self._guard.release()

    def prepare_replacement(self, source):
        self._acquire()
        try:
            revision = self.revision + 1
        finally:
            self._guard.release()
        namespaces, prefixes = source._snapshot_maps()
        return revision, namespaces, prefixes

    def reserve_replacement(self, prepared, expected_revision):
        self._acquire()
        try:
            if self.revision != expected_revision or prepared[0] != expected_revision + 1:
                raise RuntimeError("Namespace bindings changed during transaction preparation")
            self._publication = prepared
        except BaseException:
            self._guard.release()
            raise

    def publish_replacement(self, prepared):
        if self._publication is not prepared:
            raise RuntimeError("Namespace publication does not own this reservation")
        # All allocations and revision arithmetic precede native commit. These
        # independent maps cannot be changed through an escaped transaction view.
        self.revision, self._namespaces, self._prefixes = prepared
        self._publication = None
        self._guard.release()

    def cancel_replacement(self, prepared):
        if self._publication is not prepared:
            raise RuntimeError("Namespace publication does not own this reservation")
        self._publication = None
        self._guard.release()
