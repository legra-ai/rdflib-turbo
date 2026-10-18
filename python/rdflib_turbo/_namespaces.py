"""Store-owned namespace metadata with constant-time lookup in both directions."""

from rdflib import URIRef


class NamespaceBindings:
    def __init__(self, source=()):
        if isinstance(source, NamespaceBindings):
            self._namespaces = source._namespaces.copy()
            self._prefixes = source._prefixes.copy()
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
        prefix, namespace = str(prefix), URIRef(namespace)
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

    def copy(self):
        return type(self)(self)

    def replace(self, source):
        self._namespaces = source._namespaces.copy()
        self._prefixes = source._prefixes.copy()
