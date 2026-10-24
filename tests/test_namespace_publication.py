"""Prepare namespace allocations and reserve publication before native commit."""

from threading import Event, Thread

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib_turbo import Graph, StoreDataset
from rdflib_turbo._namespaces import NamespaceBindings


def source(factory):
    owner = factory()
    owner.bind("before", URIRef("urn:before:"))
    view = owner if isinstance(owner, StoreDataset) else StoreDataset(store=owner.store)
    view.default_graph.add((URIRef("urn:kept"), URIRef("urn:p"), Literal("kept")))
    view.graph(BNode("kept-empty"))
    # Initialize facade metadata before injecting the allocation failure.
    tuple(owner.namespaces())
    return owner, view


def snapshot(owner, view):
    contexts = list(view.graphs())
    return (
        {(context.identifier, *triple) for context in contexts for triple in context},
        {context.identifier for context in contexts},
        dict(owner.namespaces()),
        owner.store._namespaces.revision,
    )


def fail_copy(mapping, failure, fail_at):
    calls = []

    class FailCopy(dict):
        def copy(self):
            calls.append(None)
            if len(calls) == fail_at:
                raise failure
            return FailCopy(self)

    return FailCopy(mapping), calls


def add_pending(pending):
    pending.bind("pending", URIRef("urn:pending:"))
    pending.add((URIRef("urn:added"), URIRef("urn:p"), Literal("new")))
    StoreDataset(store=pending.store).graph(URIRef("urn:created"))


@pytest.mark.parametrize("factory", (Graph, StoreDataset))
@pytest.mark.parametrize("field", ("_namespaces", "_prefixes"))
def test_namespace_copy_failure_aborts_rdf_graph_domains_and_metadata(factory, field):
    owner, view = source(factory)
    before = snapshot(owner, view)
    identity = owner.store._namespaces
    failure = RuntimeError("injected namespace allocation failure")
    mapping, calls = fail_copy(getattr(identity, field), failure, fail_at=3)
    setattr(identity, field, mapping)

    with pytest.raises(RuntimeError) as caught:
        with owner.transaction() as pending:
            add_pending(pending)

    assert caught.value is failure
    assert len(calls) == 3
    assert owner.store._namespaces is identity
    assert snapshot(owner, view) == before
    owner.bind("after-failure", URIRef("urn:after-failure:"))


@pytest.mark.parametrize("field", ("_namespaces", "_prefixes"))
def test_delta_namespace_copy_failure_aborts_pending_inference(field):
    owner, view = source(StoreDataset)
    assert callable(getattr(owner, "infer_delta", None)), "Dataset requires atomic infer_delta"
    before = snapshot(owner, view)
    identity = owner.store._namespaces
    failure = RuntimeError("injected namespace allocation failure")
    mapping, calls = fail_copy(getattr(identity, field), failure, fail_at=2)
    setattr(identity, field, mapping)

    with pytest.raises(RuntimeError) as caught:
        owner.infer_delta(inference="rdfs")

    assert caught.value is failure
    assert len(calls) == 2
    assert snapshot(owner, view) == before
    owner.bind("after-failure", URIRef("urn:after-failure:"))


@pytest.mark.parametrize("factory", (Graph, StoreDataset))
def test_namespace_publication_preserves_owner_and_escaped_transaction_independence(factory):
    owner, _ = source(factory)
    identity = owner.store._namespaces
    with owner.transaction() as pending:
        add_pending(pending)
    expected = dict(owner.namespaces())
    assert expected["pending"] == URIRef("urn:pending:")
    assert owner.store._namespaces is identity
    assert identity._namespaces is not pending.store._namespaces._namespaces
    assert identity._prefixes is not pending.store._namespaces._prefixes
    pending.bind("escaped", URIRef("urn:escaped:"))
    assert dict(owner.namespaces()) == expected


@pytest.mark.parametrize("operation", ("graph", "dataset", "delta"))
@pytest.mark.parametrize("commit_fails", (False, True))
def test_competing_namespace_bind_fails_fast_during_native_acceptance(operation, commit_fails):
    owner, view = source(Graph if operation == "graph" else StoreDataset)
    if operation == "delta":
        assert callable(getattr(owner, "infer_delta", None)), "Dataset requires atomic infer_delta"
    request, finished = Event(), Event()
    observed = []
    before = snapshot(owner, view)
    failure = RuntimeError("injected native commit failure")

    def competing_bind():
        assert request.wait(3), "Native acceptance did not reach its observed commit boundary"
        try:
            owner.bind("competing", URIRef("urn:competing:"))
        except BaseException as cause:
            observed.append(cause)
        else:
            observed.append(None)
        finally:
            finished.set()

    def at_accept():
        request.set()
        assert finished.wait(3), "Namespace mutation blocked instead of failing fast"

    class CommitGate:
        def __init__(self, native):
            self.native = native

        def __getattr__(self, name):
            return getattr(self.native, name)

        def commit(self):
            at_accept()
            if commit_fails:
                self.native.rollback()
                raise failure
            return self.native.commit()

        def accept(self):
            at_accept()
            if commit_fails:
                self.native.close()
                raise failure
            return self.native.accept()

    native_owner = owner.store.backend._native

    class OwnerGate:
        def __getattr__(self, name):
            return getattr(native_owner, name)

        def transaction(self):
            return CommitGate(native_owner.transaction())

        def _prepare_inference_changes(self, *args, **kwargs):
            return CommitGate(native_owner._prepare_inference_changes(*args, **kwargs))

    owner.store.backend._native = OwnerGate()

    thread = Thread(target=competing_bind)
    thread.start()
    try:
        try:
            if operation == "delta":
                owner.infer_delta(inference="rdfs")
            else:
                with owner.transaction() as pending:
                    add_pending(pending)
        except RuntimeError as caught:
            assert commit_fails and caught is failure
        else:
            assert not commit_fails
    finally:
        owner.store.backend._native = native_owner
        request.set()
        thread.join(3)
    assert not thread.is_alive()
    assert len(observed) == 1
    assert isinstance(observed[0], RuntimeError), observed
    assert "Namespace" in str(observed[0])
    assert "competing" not in dict(owner.namespaces())
    if commit_fails:
        assert snapshot(owner, view) == before
    owner.bind("after-commit", URIRef("urn:after-commit:"))


def test_prefix_coercion_cannot_resume_into_reserved_publication():
    namespaces = NamespaceBindings([("before", URIRef("urn:before:"))])
    entered, resume = Event(), Event()
    failures = []

    class Prefix:
        def __str__(self):
            entered.set()
            assert resume.wait(3)
            return "competing"

    def bind():
        try:
            namespaces.bind(Prefix(), "urn:competing:")
        except BaseException as cause:
            failures.append(cause)

    thread = Thread(target=bind)
    thread.start()
    assert entered.wait(3)
    replacement = namespaces.prepare_replacement(namespaces.copy())
    namespaces.reserve_replacement(replacement, namespaces.revision)
    try:
        resume.set()
        thread.join(3)
        assert not thread.is_alive()
        assert len(failures) == 1 and isinstance(failures[0], RuntimeError)
        namespaces.publish_replacement(replacement)
    except BaseException:
        namespaces.cancel_replacement(replacement)
        raise
    finally:
        resume.set()
        thread.join(3)
    assert namespaces.namespace("competing") is None


def test_two_map_snapshot_excludes_concurrent_mutation_and_publication():
    namespaces = NamespaceBindings([("before", URIRef("urn:before:"))])
    replacement = namespaces.prepare_replacement(namespaces.copy())
    entered, resume = Event(), Event()
    copied, failures = [], []

    class PausingCopy(dict):
        def copy(self):
            entered.set()
            assert resume.wait(3)
            return dict(self)

    namespaces._namespaces = PausingCopy(namespaces._namespaces)

    def copy():
        try:
            copied.append(namespaces.copy())
        except BaseException as cause:
            failures.append(cause)

    thread = Thread(target=copy)
    thread.start()
    assert entered.wait(3)
    try:
        with pytest.raises(RuntimeError, match="Namespace"):
            namespaces.bind("during", "urn:during:")
        with pytest.raises(RuntimeError, match="Namespace"):
            namespaces.reserve_replacement(replacement, namespaces.revision)
        with pytest.raises(RuntimeError, match="Namespace"):
            namespaces.prepare_replacement(NamespaceBindings())
        assert namespaces.namespace("before") == URIRef("urn:before:")
    finally:
        resume.set()
        thread.join(3)
    assert not thread.is_alive() and not failures
    assert len(copied) == 1
    assert dict(copied[0].items()) == dict(namespaces.items())


def test_wrong_publication_token_cannot_release_another_reservation():
    namespaces = NamespaceBindings()
    staged = NamespaceBindings([("staged", URIRef("urn:staged:"))])
    prepared = namespaces.prepare_replacement(staged)
    other = namespaces.prepare_replacement(staged)
    namespaces.reserve_replacement(prepared, namespaces.revision)
    try:
        with pytest.raises(RuntimeError, match="does not own"):
            namespaces.publish_replacement(other)
        with pytest.raises(RuntimeError, match="does not own"):
            namespaces.cancel_replacement(other)
        with pytest.raises(RuntimeError, match="Namespace"):
            namespaces.bind("blocked", "urn:blocked:")
    finally:
        namespaces.cancel_replacement(prepared)
    assert namespaces.namespace("staged") is None
    namespaces.bind("released", "urn:released:")


def test_consumed_publication_and_stale_revision_fail_without_leaking_the_guard():
    namespaces = NamespaceBindings()
    staged = NamespaceBindings([("staged", URIRef("urn:staged:"))])
    prepared = namespaces.prepare_replacement(staged)
    namespaces.reserve_replacement(prepared, namespaces.revision)
    namespaces.publish_replacement(prepared)
    with pytest.raises(RuntimeError, match="changed"):
        namespaces.reserve_replacement(prepared, namespaces.revision)
    namespaces.bind("after-consumed", "urn:after-consumed:")
    prepared = namespaces.prepare_replacement(staged)
    revision = namespaces.revision
    namespaces.bind("intervening", "urn:intervening:")
    with pytest.raises(RuntimeError, match="changed"):
        namespaces.reserve_replacement(prepared, revision)
    assert namespaces.namespace("intervening") == URIRef("urn:intervening:")
    namespaces.bind("after-stale", "urn:after-stale:")
