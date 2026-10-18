"""Graph registration must use the native graph-name index, including owner views."""

import pytest
from rdflib import BNode, Dataset, Literal, URIRef
from rdflib.graph import DATASET_DEFAULT_GRAPH_ID
from rdflib_turbo import Store, StoreDataset
from rdflib_turbo._reads import encode

TRIPLE = (URIRef("urn:s"), URIRef("urn:p"), Literal("value"))
IDS = [URIRef("urn:graph"), URIRef("relative/graph"), BNode("graph")]
INVALID = [
    ("unknown", "x", None, None),
    ("uri", "x", "urn:datatype", None),
    encode(Literal("invalid")),
]


def source(identifier):
    return encode(identifier)


@pytest.mark.parametrize("identifier", IDS)
@pytest.mark.parametrize("existing", [False, True])
def test_graph_registration_never_enumerates_contexts(monkeypatch, identifier, existing):
    dataset = StoreDataset()
    dataset.default_graph.add(TRIPLE)
    if existing:
        dataset.graph(identifier).add(TRIPLE)

    def forbidden(*args, **kwargs):
        raise AssertionError("graph registration enumerated contexts")

    class IndexedReadsOnly:
        def __getattr__(self, name):
            if name in {"named_graphs", "count_pattern", "graph_len"}:
                return forbidden
            return getattr(native, name)

    native = dataset.store.backend._native
    monkeypatch.setattr(dataset.store, "contexts", forbidden)
    monkeypatch.setattr(dataset.store.backend, "_native", IndexedReadsOnly())
    graph = dataset.graph(identifier)
    monkeypatch.setattr(dataset.store.backend, "_native", native)
    assert set(graph) == ({TRIPLE} if existing else set())
    assert set(dataset.default_graph) == {TRIPLE}


@pytest.mark.parametrize("identifier", IDS)
def test_empty_and_duplicate_graph_registration_matches_reference(identifier):
    for dataset in [Dataset(), StoreDataset()]:
        dataset.default_graph.add(TRIPLE)
        graph = dataset.graph(identifier)
        assert identifier in {view.identifier for view in dataset.graphs()}
        assert len(graph) == 0
        graph.add(TRIPLE)
        assert set(dataset.graph(identifier)) == {TRIPLE}
        dataset.remove_graph(identifier)
        assert identifier not in {
            view.identifier
            for view in dataset.graphs()
            if view.identifier != DATASET_DEFAULT_GRAPH_ID
        }
        assert len(dataset.graph(identifier)) == 0
        assert set(dataset.default_graph) == {TRIPLE}


def test_indexed_membership_uses_owner_snapshot_and_preserves_existing_data(monkeypatch):
    dataset = StoreDataset()
    committed = URIRef("urn:committed")
    staged = URIRef("urn:staged")
    dataset.graph(committed).add(TRIPLE)
    native = dataset.store.backend._native
    assert native.contains_named_graph(source(committed))
    assert not native.contains_named_graph(source(staged))
    with dataset.transaction() as transaction:
        monkeypatch.setattr(
            transaction.store, "contexts", lambda *args: pytest.fail("owner enumerated contexts")
        )
        assert not transaction.store.backend._native.contains_named_graph(source(staged))
        transaction.graph(staged)
        assert transaction.store.backend._native.contains_named_graph(source(staged))
        assert not native.contains_named_graph(source(staged))
        assert set(dataset.graph(committed)) == {TRIPLE}
        with pytest.raises(RuntimeError, match="transaction"):
            dataset.graph(URIRef("urn:cannot-register-during-writer"))
        transaction.graph(committed).add(TRIPLE)
    assert native.contains_named_graph(source(staged))
    assert set(dataset.graph(committed)) == {TRIPLE}
    with pytest.raises(ValueError, match="abort"):
        with dataset.transaction() as transaction:
            transaction.graph(URIRef("urn:aborted"))
            raise ValueError("abort")
    assert not native.contains_named_graph(source(URIRef("urn:aborted")))


@pytest.mark.parametrize("invalid", INVALID)
def test_invalid_owner_membership_aborts_even_if_error_is_caught(invalid):
    dataset = StoreDataset()
    with pytest.raises(RuntimeError, match=r"closed|finished"):
        with dataset.transaction() as transaction:
            transaction.graph(URIRef("urn:staged"))
            with pytest.raises(ValueError):
                transaction.store.backend._native.contains_named_graph(invalid)
    assert not dataset.store.backend._native.contains_named_graph(source(URIRef("urn:staged")))
    dataset.graph(URIRef("urn:next-writer"))


def test_membership_and_named_views_reject_closed_handles():
    dataset = StoreDataset()
    with dataset.transaction() as transaction:
        transaction.graph(URIRef("urn:graph"))
    with pytest.raises(RuntimeError, match=r"closed|finished"):
        transaction.graph(URIRef("urn:graph"))
    dataset.close()
    with pytest.raises(RuntimeError, match="closed"):
        dataset.graph(URIRef("urn:graph"))


def test_empty_named_graph_membership_survives_persistent_reopen(tmp_path):
    identifier = BNode("persistent-empty")
    with Store(tmp_path / "store") as backend:
        dataset = StoreDataset(backend)
        dataset.graph(identifier)
        assert backend._native.contains_named_graph(source(identifier))
    with Store(tmp_path / "store") as backend:
        assert backend._native.contains_named_graph(source(identifier))
        assert len(StoreDataset(backend).graph(identifier)) == 0


@pytest.mark.parametrize("invalid", INVALID)
def test_invalid_committed_membership_does_not_reserve_writer(invalid):
    dataset = StoreDataset()
    with pytest.raises(ValueError):
        dataset.store.backend._native.contains_named_graph(invalid)
    dataset.graph(URIRef("urn:after-invalid-read"))
