"""Namespace lookups stay indexed and transaction views keep both directions."""

import pytest
from rdflib import URIRef
from rdflib.plugins.stores.memory import Memory
from rdflib_turbo import Graph, Store
from rdflib_turbo.graph import RDFLibStore


class ReadGuard(dict):
    forbidden = False

    def items(self):
        if self.forbidden:
            raise AssertionError("A namespace lookup scanned all bindings")
        return super().items()


def test_prefix_lookup_does_not_scan_namespace_bindings():
    bindings = ReadGuard({f"p{i}": URIRef(f"urn:namespace:{i}:") for i in range(128)})
    store = RDFLibStore(namespaces=bindings)
    bindings.forbidden = True
    assert store.prefix(URIRef("urn:namespace:127:")) == "p127"
    assert store.prefix(URIRef("urn:missing:")) is None


def test_namespace_bindings_match_reference_in_both_directions():
    expected, actual = Memory(), RDFLibStore()
    first, second, third = map(URIRef, ("urn:first:", "urn:second:", "urn:third:"))
    for prefix, namespace, override in (
        ("a", first, True),
        ("b", second, True),
        ("a", second, True),
        ("c", second, False),
        ("c", third, False),
        ("a", third, True),
        ("z", first, True),
    ):
        expected.bind(prefix, namespace, override=override)
        actual.bind(prefix, namespace, override=override)
        assert dict(actual.namespaces()) == dict(expected.namespaces())
        for value in (first, second, third, URIRef("urn:absent:")):
            assert actual.prefix(value) == expected.prefix(value)
        for value in ("a", "b", "c", "z", "absent"):
            assert actual.namespace(value) == expected.namespace(value)


@pytest.mark.parametrize("abort", [False, True])
def test_namespace_transaction_indexes_commit_or_rollback_together(abort):
    backend = Store()
    graph = Graph(store=backend, bind_namespaces="none")
    other = Graph(store=backend, bind_namespaces="none")
    old, new = URIRef("urn:old:"), URIRef("urn:new:")
    graph.bind("p", old)

    def execute():
        with graph.transaction() as transaction:
            transaction.bind("p", new, replace=True)
            assert transaction.store.prefix(new) == "p"
            assert transaction.store.prefix(old) is None
            assert other.store.prefix(old) == "p"
            assert other.store.prefix(new) is None
            if abort:
                raise ValueError("abort transaction")

    if abort:
        with pytest.raises(ValueError, match="abort transaction"):
            execute()
    else:
        execute()
    expected = old if abort else new
    removed = new if abort else old
    for view in (graph, other):
        assert view.store.namespace("p") == expected
        assert view.store.prefix(expected) == "p"
        assert view.store.prefix(removed) is None
