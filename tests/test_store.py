"""Transactions operate on the native store, without copying the base dataset."""

import json

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib.namespace import XSD
from rdflib_turbo import Store, StoreDataset

TRIPLE = '<urn:s> <urn:p> "01"^^<http://www.w3.org/2001/XMLSchema#integer> .'
INSERT = "INSERT DATA { " + TRIPLE + " }"


def test_store_parse_query_and_update_preserve_exact_terms():
    with Store() as store:
        assert len(store) == 0
        assert store.parse(data=TRIPLE) is store
        value = next(iter(store.query("SELECT ?o WHERE { ?s ?p ?o }"))).o
        assert value == Literal("01", datatype=XSD.integer, normalize=False)
        assert store.query("ASK { ?s ?p ?o }").askAnswer
        assert '"01"' in store.serialize()
        assert (
            store.update(
                'INSERT DATA { <urn:s> <urn:p> "1"^^<http://www.w3.org/2001/XMLSchema#integer> }'
            )
            is store
        )
        assert len(store) == 2
        assert len(store.query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }").graph) == 2
        store.update("DELETE DATA { " + TRIPLE + " }")
        assert len(store) == 1


def test_transaction_reads_its_writes_and_commits_together():
    with Store() as store:
        with store.transaction() as transaction:
            assert transaction.parse(data=TRIPLE) is transaction
            assert (
                transaction.update("INSERT DATA { GRAPH <urn:g> { <urn:a> <urn:b> <urn:c> } }")
                is transaction
            )
            assert len(transaction) == 2
            assert len(store) == 0
            assert next(iter(transaction.query("SELECT ?o WHERE { ?s ?p ?o }"))).o == Literal(
                "01", datatype=XSD.integer, normalize=False
            )
            assert transaction.query("ASK { GRAPH <urn:g> { ?s ?p ?o } }").askAnswer
            assert not store.query("ASK { ?s ?p ?o }").askAnswer
            assert "urn:g" in transaction.serialize()
        assert len(store) == 2
        with pytest.raises(RuntimeError, match="closed"):
            len(transaction)


def test_transaction_error_aborts_insertions_and_deletions():
    with Store() as store:
        store.parse(data=TRIPLE)
        with pytest.raises(ValueError, match="stop"):
            with store.transaction() as transaction:
                transaction.update("DELETE WHERE { ?s ?p ?o }")
                transaction.update("INSERT DATA { <urn:a> <urn:b> <urn:c> }")
                raise ValueError("stop")
        assert len(store) == 1
        assert '"01"' in store.serialize()
        with store.transaction() as transaction:
            transaction.update("INSERT DATA { <urn:a> <urn:b> <urn:c> }")
        assert len(store) == 2


def test_malformed_parse_never_commits_partial_data():
    with Store() as store:
        with pytest.raises(ValueError):
            store.parse(data=TRIPLE + " <urn:broken>")
        assert len(store) == 0
        with pytest.raises(RuntimeError, match=r"closed|failed"):
            with store.transaction() as transaction:
                transaction.parse(data=TRIPLE)
                with pytest.raises(ValueError):
                    transaction.parse(data="<urn:broken>")
        assert len(store) == 0


def test_failed_update_aborts_the_whole_transaction():
    with Store() as store:
        with pytest.raises(ValueError):
            with store.transaction() as transaction:
                transaction.update(INSERT)
                transaction.update("INVALID UPDATE")
        assert len(store) == 0
        store.update(INSERT)
        assert len(store) == 1


def test_active_writer_rejects_conflicting_operations_without_deadlock():
    with Store() as store:
        with store.transaction():
            for operation in [
                lambda: store.update(INSERT),
                lambda: store.parse(data=TRIPLE),
                store.close,
            ]:
                with pytest.raises(RuntimeError, match="active transaction"):
                    operation()
            with pytest.raises(RuntimeError, match="active transaction"):
                with store.transaction():
                    pass
        assert len(store) == 0


def test_persistence_reopen_preserves_commits_and_aborts(tmp_path):
    path = tmp_path / "database"
    with Store(path) as store:
        store.update(INSERT)
        with pytest.raises(ValueError):
            with store.transaction() as transaction:
                transaction.update("DELETE WHERE { ?s ?p ?o }")
                raise ValueError("abort")
    with Store(path) as store:
        assert len(store) == 1
        assert '"01"' in store.serialize()
        store.update("INSERT DATA { GRAPH <urn:g> { <urn:a> <urn:b> <urn:c> } }")
    with Store(path) as store:
        assert len(store) == 2
        assert store.query("ASK { GRAPH <urn:g> { ?s ?p ?o } }").askAnswer


def test_closed_store_fails_explicitly():
    store = Store()
    store.close()
    store.close()
    for operation in [
        lambda: len(store),
        lambda: store.query("ASK {}"),
        lambda: store.update(INSERT),
        lambda: store.parse(data=TRIPLE),
        lambda: store.serialize(),
    ]:
        with pytest.raises(RuntimeError, match="closed"):
            operation()
    with pytest.raises(RuntimeError, match="closed"):
        with store.transaction():
            pass
    with pytest.raises(RuntimeError, match="closed"):
        with store:
            pass


def test_store_union_and_graph_only_serialization():
    with Store() as store:
        store.parse(data="<urn:s> <urn:p> <urn:o> <urn:g> .", format="nquads")
        assert not store.query("ASK { ?s ?p ?o }").askAnswer
        assert store.query("ASK { ?s ?p ?o }", default_union=True).askAnswer
        with pytest.raises(ValueError):
            store.serialize(format="nt")
        with store.transaction() as transaction:
            assert transaction.query("ASK { ?s ?p ?o }", default_union=True).askAnswer


def test_store_path_parsing_and_graph_queries(tmp_path):
    path = tmp_path / "data.nt"
    path.write_text(TRIPLE)
    with Store() as store:
        store.parse(path, format="nt")
        graph = store.query("DESCRIBE <urn:s>").graph
        assert graph.value(URIRef("urn:s"), URIRef("urn:p")) == Literal(
            "01", datatype=XSD.integer, normalize=False
        )


def test_transaction_accepts_calls_from_another_thread():
    from concurrent.futures import ThreadPoolExecutor

    with Store() as store:
        with store.transaction() as transaction:
            with ThreadPoolExecutor(max_workers=1) as executor:
                assert executor.submit(transaction.update, INSERT).result(timeout=10) is transaction
            assert len(transaction) == 1
            assert len(store) == 0
        assert len(store) == 1


@pytest.mark.parametrize(
    "operation",
    [
        lambda transaction: transaction.query("INVALID QUERY"),
        lambda transaction: transaction.parse(data=TRIPLE, format="invalid-format"),
        lambda transaction: transaction.serialize(format="invalid-format"),
        lambda transaction: transaction._native.load_graph(TRIPLE, "invalid-format", None, ""),
        lambda transaction: transaction._native.change_graph("", False),
        lambda transaction: transaction.update(
            "INSERT DATA { <urn:a> <urn:b> <urn:c> }; LOAD <https://example.invalid/data>"
        ),
    ],
)
def test_caught_native_errors_doom_the_transaction(operation):
    with Store() as store:
        with pytest.raises(RuntimeError, match=r"closed|failed"):
            with store.transaction() as transaction:
                transaction.update(INSERT)
                with pytest.raises((ValueError, RuntimeError, NotImplementedError)):
                    operation(transaction)
        assert len(store) == 0


def test_blank_node_contexts_commit_and_abort_with_dataset_transactions():
    dataset = StoreDataset()
    original, committed, aborted = map(BNode, ("original", "committed", "aborted"))
    triple = (URIRef("urn:s"), URIRef("urn:p"), Literal("value"))
    dataset.graph(original).add(triple)
    with dataset.transaction() as transaction:
        assert set(transaction.graph(original)) == {triple}
        transaction.graph(committed).add(triple)
        transaction.remove_graph(original)
        assert not list(transaction.graph(original))
        assert set(dataset.graph(original)) == {triple}
    assert set(dataset.graph(committed)) == {triple}
    assert not list(dataset.graph(original))
    with pytest.raises(ValueError, match="abort named context"):
        with dataset.transaction() as transaction:
            transaction.graph(aborted).add(triple)
            transaction.remove_graph(committed)
            raise ValueError("abort named context")
    assert set(dataset.graph(committed)) == {triple}
    assert not list(dataset.graph(aborted))


@pytest.mark.parametrize(
    "pattern",
    [
        [{"type": "literal", "value": "bad-subject"}, None, None, None],
        [None, {"type": "bnode", "value": "bad-predicate"}, None, None],
        [None, None, {"type": "unknown", "value": "bad-object"}, None],
        [None, None, None, json.dumps({"type": "literal", "value": "bad-graph"})],
    ],
)
def test_invalid_native_pattern_transport_aborts_prior_writes(pattern):
    with Store() as store:
        with pytest.raises(RuntimeError, match=r"closed|failed"):
            with store.transaction() as transaction:
                transaction.update(INSERT)
                with pytest.raises(ValueError):
                    transaction._native.match_quads(json.dumps(pattern))
        assert len(store) == 0
        store.update(INSERT)
        assert len(store) == 1


@pytest.mark.parametrize("populated", [False, True])
def test_store_triples_scoped_protocol_returns_the_requested_context(populated):
    dataset = StoreDataset()
    graph = dataset.graph(BNode("requested-context"))
    triple = (URIRef("urn:s"), URIRef("urn:p"), Literal("value"))
    if populated:
        graph.add(triple)
    rows = list(dataset.store.triples((None, None, None), context=graph))
    if not populated:
        assert rows == []
        return
    assert len(rows) == 1
    actual, contexts = rows[0]
    assert actual == triple
    assert iter(contexts) is contexts
    assert list(contexts) == [graph]
