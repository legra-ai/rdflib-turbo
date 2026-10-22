"""Installed-wheel Graph contracts; an original Graph re-export must fail."""

import pytest
import rdflib_turbo
from rdflib import BNode, Literal, Namespace, URIRef, Variable
from rdflib import Graph as ReferenceGraph

EX = Namespace("urn:example:")


def test_graph_is_actual_native_facade_not_reference_export():
    assert "site-packages" in rdflib_turbo.__file__
    assert rdflib_turbo.Graph is not ReferenceGraph
    graph = rdflib_turbo.Graph()
    assert graph.store.__class__.__module__ == "rdflib_turbo.graph"
    assert type(graph.store.backend._native).__module__ == "rdflib_turbo._native"
    graph.add(
        (
            EX.s,
            EX.p,
            Literal(
                "01", datatype=URIRef("http://www.w3.org/2001/XMLSchema#integer"), normalize=False
            ),
        )
    )
    assert len(graph) == 1
    assert next(iter(graph.objects(EX.s, EX.p))).value == 1
    assert str(next(iter(graph.objects(EX.s, EX.p)))) == "01"


def test_graph_subclassing_blank_identity_and_basic_iteration():
    class ApplicationGraph(rdflib_turbo.Graph):
        pass

    graph = ApplicationGraph()
    blank = BNode("caller-label")
    graph.add((blank, EX.p, Literal("v"))).add((EX.s, EX.p, blank))
    assert set(graph.subjects(EX.p, Literal("v"))) == {blank}
    assert set(graph.predicate_objects(blank)) == {(EX.p, Literal("v"))}
    assert set(graph.subject_objects(EX.p)) == {(blank, Literal("v")), (EX.s, blank)}
    assert len(list(graph)) == 2
    graph.remove((blank, None, None))
    assert list(graph) == [(EX.s, EX.p, blank)]


def test_namespace_manager_and_rdf_parse_serialize():
    graph = rdflib_turbo.Graph()
    graph.namespace_manager.bind("ex", EX)
    assert ("ex", URIRef(EX)) in set(graph.namespaces())
    assert graph.namespace_manager.normalizeUri(EX.s) == "ex:s"
    assert (
        graph.parse(data='@prefix ex: <urn:example:> . ex:s ex:p "v" .', format="turtle") is graph
    )
    restored = rdflib_turbo.Graph().parse(data=graph.serialize(format="turtle"), format="turtle")
    assert graph.isomorphic(restored)


def test_native_parser_preserves_exact_literal_spelling_and_document_blank_scope():
    graph = rdflib_turbo.Graph()
    document = '_:a <urn:p> "01"^^<http://www.w3.org/2001/XMLSchema#integer> .'
    graph.parse(data=document, format="nt").parse(data=document, format="nt")
    assert len(graph) == 2
    assert {str(value) for value in graph.objects()} == {"01"}


def test_existing_rdflib_terms_are_not_reparsed_as_rdf_documents():
    graph = rdflib_turbo.Graph()
    obj = URIRef("preffix-._1.:suffix-._1")
    graph.add((EX.s, EX.p, obj))
    assert set(graph) == {(EX.s, EX.p, obj)}
    graph.remove((EX.s, EX.p, obj))
    assert len(graph) == 0


def test_native_query_never_uses_reference_evaluator(monkeypatch):
    def forbidden(*args, **kwargs):
        raise AssertionError("reference SPARQL evaluator executed")

    import rdflib.plugins.sparql.processor

    monkeypatch.setattr(rdflib.plugins.sparql.processor.SPARQLProcessor, "query", forbidden)
    graph = rdflib_turbo.Graph()
    graph.add((EX.s, EX.p, Literal("v")))
    assert list(graph.query('SELECT ?s WHERE { ?s <urn:example:p> "v" }')) == [(EX.s,)]
    graph.update('INSERT DATA { <urn:example:s2> <urn:example:p> "v2" }')
    assert len(graph) == 2


def test_store_dataset_contexts_and_transaction_scope():
    dataset = rdflib_turbo.StoreDataset()
    dataset.default_graph.add((EX.s, EX.p, Literal("default")))
    dataset.graph(EX.named).add((EX.s, EX.p, Literal("named")))
    assert len(list(dataset.quads((None, None, None, None)))) == 2
    assert set(dataset.graph(EX.named).objects()) == {Literal("named")}
    with dataset.transaction() as transaction:
        transaction.graph(EX.named).add((EX.other, EX.p, Literal("new")))
        assert len(transaction.graph(EX.named)) == 2
        assert len(dataset.graph(EX.named)) == 1
    assert len(dataset.graph(EX.named)) == 2
    with pytest.raises(ValueError, match="abort"):
        with dataset.transaction() as transaction:
            transaction.default_graph.remove((None, None, None))
            raise ValueError("abort")
    assert len(dataset.default_graph) == 1


def test_no_reference_memory_store_allocated(monkeypatch):
    from rdflib.plugins.stores.memory import Memory

    def forbidden(*args, **kwargs):
        raise AssertionError("Reference memory storage was allocated")

    monkeypatch.setattr(Memory, "__init__", forbidden)
    graph = rdflib_turbo.Graph().parse(data='<urn:s> <urn:p> "v" .', format="nt")
    result = graph.query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }")
    assert isinstance(result.graph, rdflib_turbo.Graph)
    assert list(result.graph) == [(URIRef("urn:s"), URIRef("urn:p"), Literal("v"))]
    assert "urn:s" in graph.serialize(format="nt")


def test_scoped_named_graph_queries_and_empty_contexts():
    dataset = rdflib_turbo.StoreDataset()
    named = dataset.graph(EX.named)
    assert EX.named in {g.identifier for g in dataset.graphs()}
    named.add((EX.s, EX.p, Literal("named")))
    dataset.default_graph.add((EX.s, EX.p, Literal("default")))
    assert list(named.query("SELECT ?o WHERE { ?s ?p ?o }")) == [(Literal("named"),)]
    assert list(dataset.query("SELECT ?o WHERE { ?s ?p ?o }")) == [(Literal("default"),)]
    with dataset.transaction() as transaction:
        transaction.graph(EX.named).add((EX.other, EX.p, Literal("new")))
        assert len(transaction.graph(EX.named).query("SELECT ?o WHERE { ?s ?p ?o }")) == 2
        assert len(named) == 1


def test_parse_is_atomic_and_namespace_bindings_rollback():
    graph = rdflib_turbo.Graph()
    graph.add((EX.s, EX.p, Literal("before")))
    namespaces = set(graph.namespaces())
    with pytest.raises(ValueError):
        graph.parse(
            data='@prefix failed: <urn:failed:> . failed:s failed:p "new" . BROKEN', format="turtle"
        )
    assert len(graph) == 1
    assert set(graph.namespaces()) == namespaces


def test_caught_parse_failure_aborts_explicit_transaction():
    graph = rdflib_turbo.Graph()
    with pytest.raises(RuntimeError, match="closed"):
        with graph.transaction() as transaction:
            transaction.add((EX.s, EX.p, Literal("new")))
            try:
                transaction.parse(data='<urn:s> <urn:p> "okay" . BROKEN', format="nt")
            except ValueError:
                pass
    assert len(graph) == 0


def test_unsupported_processors_never_execute_reference_fallback():
    graph = rdflib_turbo.Graph()
    with pytest.raises(ValueError, match="native"):
        graph.query("ASK {}", use_store_provided=False)
    assert bool(graph.query("ASK {}", initBindings={"s": EX.s}))


def test_length_does_not_materialize_python_triple_results(monkeypatch):
    graph = rdflib_turbo.Graph()
    graph.add((EX.s, EX.p, Literal("v")))

    def forbidden(*args, **kwargs):
        raise AssertionError("Length materialized triples in Python")

    monkeypatch.setattr(graph.store, "triples", forbidden)
    assert len(graph) == 1


def test_store_dataset_native_parse_and_update_share_contexts_atomically():
    dataset = rdflib_turbo.StoreDataset()
    dataset.parse(
        data='<urn:s> <urn:p> "default" . <urn:g> { <urn:s> <urn:p> "named" }', format="trig"
    )
    assert len(dataset.default_graph) == 1
    assert list(dataset.graph(URIRef("urn:g")).objects()) == [Literal("named")]
    assert len(list(dataset.quads())) == 2
    dataset.update('INSERT DATA { <urn:s2> <urn:p> "more" }')
    assert len(dataset.default_graph) == 2
    with pytest.raises(ValueError):
        dataset.parse(data='<urn:g> { <urn:broken> <urn:p> "partial" } BROKEN', format="trig")
    assert len(dataset.graph(URIRef("urn:g"))) == 1
    assert len(dataset) == 3


def test_identifier_strings_and_union_duplicate_counts():
    graph = rdflib_turbo.Graph(identifier="urn:identifier")
    graph.add((EX.s, EX.p, Literal("v")))
    graph.update('INSERT DATA { <urn:s> <urn:p> "v2" }')
    assert len(graph) == 2
    dataset = rdflib_turbo.StoreDataset(default_union=True)
    dataset.default_graph.add((EX.s, EX.p, Literal("same")))
    dataset.graph(EX.named).add((EX.s, EX.p, Literal("same")))
    assert len(dataset) == 1
    assert len(list(dataset.quads())) == 2
    assert len(dataset.query("SELECT ?s WHERE { ?s ?p ?o }")) == 1


def test_graph_unions_deduplicate_triples_before_evaluation_but_keep_solution_bags():
    dataset = rdflib_turbo.StoreDataset(default_union=True)
    dataset.graph(URIRef("urn:g1")).add((EX.s, EX.p, Literal("same")))
    dataset.graph(URIRef("urn:g2")).add((EX.s, EX.p, Literal("same")))
    dataset.graph(URIRef("urn:g2")).add((EX.other, EX.p, Literal("same")))
    assert list(dataset.query("SELECT ?o WHERE { ?s ?p ?o }")) == [(Literal("same"),)] * 2
    query = "SELECT ?o FROM <urn:g1> FROM <urn:g2> WHERE { ?s ?p ?o }"
    assert list(dataset.query(query)) == [(Literal("same"),)] * 2
    assert list(dataset.graph(URIRef("urn:g1")).query(query)) == [(Literal("same"),)] * 2


def test_graphs_over_one_explicit_store_have_distinct_contexts_and_shared_namespaces():
    store = rdflib_turbo.Store()
    first = rdflib_turbo.Graph(store=store, identifier=URIRef("urn:first"))
    second = rdflib_turbo.Graph(store=store, identifier=URIRef("urn:second"))
    first.bind("example", EX)
    first.add((EX.s, EX.p, Literal("first")))
    assert len(second) == 0
    assert ("example", URIRef(EX)) in set(second.namespaces())
    second.add((EX.s, EX.p, Literal("second")))
    assert list(first.objects()) == [Literal("first")]
    assert list(second.objects()) == [Literal("second")]


def test_namespace_rebinding_matches_original_store_contract():
    original, native = ReferenceGraph(), rdflib_turbo.Graph()
    operations = [
        ("a", URIRef("urn:namespace:first:"), {}),
        ("b", URIRef("urn:namespace:first:"), {"override": True}),
        ("b", URIRef("urn:namespace:second:"), {"replace": True}),
        ("c", URIRef("urn:namespace:second:"), {"override": False}),
    ]
    for prefix, namespace, options in operations:
        original.bind(prefix, namespace, **options)
        native.bind(prefix, namespace, **options)
        assert set(native.namespaces()) == set(original.namespaces())
        assert native.namespace_manager.normalizeUri(
            namespace + "term"
        ) == original.namespace_manager.normalizeUri(namespace + "term")


def test_language_literals_and_invalid_term_kinds():
    graph = rdflib_turbo.Graph()
    value = Literal("bonjour", lang="fr")
    graph.add((EX.s, EX.p, value))
    assert list(graph.objects()) == [value]
    with pytest.raises(TypeError, match="Unsupported RDF term"):
        graph.add((Variable("s"), EX.p, value))


def test_constructor_boundaries_and_legacy_conversion_reject_native_graphs():
    from rdflib_turbo.graph import RDFLibStore

    with pytest.raises(TypeError, match="Native Store"):
        RDFLibStore(object())
    for factory in (rdflib_turbo.Graph, rdflib_turbo.StoreDataset):
        with pytest.raises(TypeError, match="native"):
            factory(store="Memory")
        with pytest.raises(NotImplementedError, match="Store inference"):
            rdflib_turbo.Dataset.from_rdflib(factory())
    backend = rdflib_turbo.Store()
    dataset = rdflib_turbo.StoreDataset(store=backend)
    dataset.graph(EX.named).add((EX.s, EX.p, Literal("shared")))
    assert len(backend) == 1


def test_context_selection_and_removal_preserve_other_graphs():
    dataset = rdflib_turbo.StoreDataset()
    triple = (EX.s, EX.p, Literal("v"))
    dataset.default_graph.add(triple)
    dataset.graph(EX.named).add(triple)
    assert {g.identifier for g in dataset.store.contexts(triple)} == {
        dataset.default_graph.identifier,
        EX.named,
    }
    dataset.remove_graph(EX.named)
    assert list(dataset.default_graph) == [triple]
    dataset.remove_graph(dataset.default_graph)
    assert len(dataset) == 0
    with pytest.raises(ValueError, match="Quoted"):
        dataset.get_context(EX.named, quoted=True)
    with pytest.raises(ValueError, match="Quoted"):
        dataset.store.add(triple, dataset.default_graph, quoted=True)


@pytest.mark.parametrize("factory", [rdflib_turbo.Graph, rdflib_turbo.StoreDataset])
def test_query_and_update_options_fail_without_python_fallback(factory):
    graph = factory()
    assert bool(graph.query("ASK {}", initNs={}, initBindings={}))
    for options in ({"processor": "custom"}, {"result": "custom"}, {"use_store_provided": False}):
        with pytest.raises(ValueError, match="native"):
            graph.query("ASK {}", **options)
    for options in ({"processor": "custom"}, {"use_store_provided": False}):
        with pytest.raises(ValueError, match="native"):
            graph.update("INSERT DATA {}", **options)
    with pytest.raises(TypeError, match="query string"):
        graph.query(object())
    with pytest.raises(TypeError, match="query options"):
        graph.query("ASK {}", unexpected=True)
    assert bool(graph.query("ASK {}", initBindings={"s": EX.s}))
    assert graph.update("INSERT DATA {}", initBindings={"s": EX.s}) is None
    graph.update("INSERT DATA {}", initNs={}, initBindings={})


def test_store_query_default_scope_and_named_update():
    dataset = rdflib_turbo.StoreDataset()
    assert bool(dataset.store.query("ASK {}", {}, {}, None))
    assert dataset.graph(EX.named).update("INSERT DATA {}") is None


def test_close_and_manual_transaction_controls_are_explicit():
    graph = rdflib_turbo.Graph()
    with pytest.raises(RuntimeError, match="scoped context"):
        graph.commit()
    with pytest.raises(RuntimeError, match="scoped context"):
        graph.rollback()
    with pytest.raises(ValueError, match="cannot commit"):
        graph.close(commit_pending_transaction=True)
    with graph.transaction() as transaction:
        with pytest.raises(RuntimeError, match="enclosing transaction"):
            transaction.close()
        with pytest.raises(RuntimeError, match="Nested"):
            with transaction.transaction():
                pytest.fail("Nested transaction entered")
    graph.close()
    with pytest.raises(RuntimeError, match="closed"):
        len(graph)
    dataset = rdflib_turbo.StoreDataset()
    with dataset.transaction() as transaction:
        with pytest.raises(RuntimeError, match="Nested"):
            with transaction.transaction():
                pytest.fail("Nested transaction entered")


def test_parser_input_forms_and_unsupported_options(tmp_path):
    import io

    graph = rdflib_turbo.Graph()
    with pytest.raises(TypeError, match="parser options"):
        graph.parse(data="", format="nt", unexpected=True)
    path = tmp_path / "source.ttl"
    path.write_text('<urn:s> <urn:p> "v" .')
    graph.parse(source=path)
    graph.parse(source=io.BytesIO(b'<urn:s2> <urn:p> "v2" .'), format="nt")
    graph.parse(data='<urn:s3> <urn:p> "v3" .')
    assert len(graph) == 3


def test_direct_store_namespace_binding_override_contract():
    graph = rdflib_turbo.Graph(bind_namespaces="none")
    store = graph.store
    store.bind("a", EX, override=False)
    store.bind("a", URIRef("urn:ignored:"), override=False)
    assert store.namespace("a") == URIRef(EX)
    assert store.prefix(URIRef("urn:missing:")) is None
