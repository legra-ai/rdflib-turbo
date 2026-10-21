"""RDFLib initial query bindings remain native and preserve projection scopes."""

import pytest
from rdflib import BNode, Literal, Namespace, Variable
from rdflib import Graph as ReferenceGraph
from rdflib.namespace import XSD
from rdflib_turbo import Graph, Store, StoreDataset

EX = Namespace("urn:bindings:")
QUERIES = [
    "SELECT ?label WHERE { ?s <urn:bindings:label> ?label }",
    "SELECT ?label WHERE { { SELECT ?label WHERE { ?s <urn:bindings:label> ?label } } }",
    "SELECT (?s AS ?alias) WHERE {}",
    "SELECT ?s WHERE { VALUES ?s { <urn:bindings:b> } }",
    "SELECT ?s WHERE { BIND (<urn:bindings:b> AS ?s) }",
    "SELECT ?s WHERE { { SELECT ?s WHERE { BIND (<urn:bindings:b> AS ?s) } } }",
    "SELECT ?s WHERE { VALUES ?s { <urn:bindings:a> } { SELECT ?s WHERE { BIND(<urn:bindings:b> AS ?s) } } }",
    "SELECT ?s WHERE { BIND(<urn:bindings:a> AS ?s) { SELECT ?s WHERE { BIND(<urn:bindings:b> AS ?s) } } }",
    "SELECT ?s WHERE { VALUES ?s { <urn:bindings:a> } { SELECT ?s WHERE { VALUES ?s { <urn:bindings:b> } } } }",
    "SELECT ?s WHERE { VALUES ?s { <urn:bindings:a> } OPTIONAL { { SELECT ?s WHERE { BIND(<urn:bindings:b> AS ?s) } } } }",
    "SELECT ?s WHERE { VALUES ?s { <urn:bindings:a> } { SELECT DISTINCT ?s WHERE { BIND(<urn:bindings:b> AS ?s) } } }",
    "SELECT ?s WHERE { VALUES ?s { <urn:bindings:a> } { SELECT ?s WHERE { BIND(<urn:bindings:b> AS ?s) } LIMIT 1 } }",
    "SELECT ?label WHERE { VALUES ?s { <urn:bindings:a> } OPTIONAL { { SELECT ?label WHERE { ?s <urn:bindings:label> ?label } } } }",
    "ASK { ?s <urn:bindings:label> ?label }",
    "CONSTRUCT { ?s <urn:bindings:copy> ?label } WHERE { ?s <urn:bindings:label> ?label }",
    "DESCRIBE ?s WHERE { ?s <urn:bindings:label> ?label }",
]


def fixture(graph):
    graph.add((EX.a, EX.label, Literal("A")))
    graph.add((EX.b, EX.label, Literal("B")))
    return graph


def result_value(result):
    if result.type == "ASK":
        return bool(result)
    if result.type == "SELECT":
        return result.vars, sorted(tuple(row) for row in result)
    return set(result.graph)


@pytest.mark.parametrize("query", QUERIES)
@pytest.mark.parametrize("key", ["s", "?s", Variable("s")])
def test_initial_bindings_match_reference_without_exposing_hidden_variables(query, key):
    expected = fixture(ReferenceGraph()).query(query, initBindings={key: EX.a})
    actual = fixture(Graph()).query(query, initBindings={key: EX.a})
    assert result_value(actual) == result_value(expected)


@pytest.mark.parametrize("query", QUERIES)
def test_unused_bindings_preserve_correlated_algebra_semantics(query):
    bindings = {"unused": EX.a}
    expected = fixture(ReferenceGraph()).query(query, initBindings=bindings)
    actual = fixture(Graph()).query(query, initBindings=bindings)
    assert result_value(actual) == result_value(expected)


def test_unused_initial_binding_does_not_restrict_or_extend_projection():
    query = QUERIES[0]
    expected = fixture(ReferenceGraph()).query(query, initBindings={"unused": EX.a})
    actual = fixture(Graph()).query(query, initBindings={"unused": EX.a})
    assert result_value(actual) == result_value(expected)


def test_bindings_read_pending_scoped_transaction_without_other_graphs():
    with Store() as store:
        fixture(Graph(store=store, identifier=EX.outside))
        with store.transaction() as transaction:
            graph = Graph(store=transaction, identifier=EX.input)
            graph.add((EX.a, EX.label, Literal("pending")))
            assert list(graph.query(QUERIES[0], initBindings={"s": EX.a})) == [
                (Literal("pending"),)
            ]


@pytest.mark.parametrize("union", [False, True])
def test_dataset_binding_respects_default_union(union):
    dataset = StoreDataset(default_union=union)
    fixture(dataset.graph(EX.input))
    values = list(dataset.query(QUERIES[0], initBindings={"s": EX.a}))
    assert values == ([(Literal("A"),)] if union else [])


@pytest.mark.parametrize("bindings", [{}, {"unused": EX.a}])
def test_global_unused_binding_participates_in_minus_domain(bindings):
    query = "SELECT ?x WHERE { VALUES ?x {1} MINUS { VALUES ?y {2} } }"
    expected = ReferenceGraph().query(query, initBindings=bindings)
    actual = Graph().query(query, initBindings=bindings)
    assert result_value(actual) == result_value(expected)


@pytest.mark.parametrize(
    "term",
    [
        EX.a,
        BNode("binding"),
        Literal('" } UNION { ?s ?p ?o } #'),
        Literal("01", datatype=XSD.integer, normalize=False),
        Literal("hi", lang="en"),
    ],
)
def test_initial_binding_preserves_exact_term_without_source_interpolation(term):
    query = "SELECT (?input AS ?output) WHERE {}"
    actual = Graph().query(query, initBindings={"input": term})
    assert list(actual) == [(term,)]


@pytest.mark.parametrize("transactional", [False, True])
def test_invalid_python_binding_fails_before_query_and_aborts_transaction(transactional):
    with Store() as store:
        if transactional:
            with pytest.raises(RuntimeError, match="closed"):
                with store.transaction() as transaction:
                    graph = Graph(store=transaction)
                    graph.add((EX.a, EX.label, Literal("pending")))
                    with pytest.raises(TypeError, match="Unsupported RDF term"):
                        graph.query("ASK {}", initBindings={"input": object()})
                    graph.query("ASK {}")
            assert len(Graph(store=store)) == 0
        else:
            with pytest.raises(TypeError, match="Unsupported RDF term"):
                Graph(store=store).query("ASK {}", initBindings={"input": object()})


def test_invalid_native_binding_aborts_even_if_caught():
    with Store() as store:
        with pytest.raises(RuntimeError, match="closed"):
            with store.transaction() as transaction:
                graph = Graph(store=transaction)
                graph.add((EX.a, EX.label, Literal("pending")))
                with pytest.raises(ValueError):
                    transaction._native.query(
                        "ASK {}", False, '{"s":{"type":"invalid","value":"x"}}'
                    )
                transaction._native.query("ASK {}")
        assert len(Graph(store=store)) == 0


def test_initial_bindings_do_not_invoke_python_query_evaluator(monkeypatch):
    import rdflib.plugins.sparql.processor

    def forbidden(*args, **kwargs):
        raise AssertionError("Python SPARQL execution used")

    monkeypatch.setattr(rdflib.plugins.sparql.processor, "evalQuery", forbidden)
    actual = fixture(Graph()).query(QUERIES[0], initBindings={"s": EX.a})
    assert list(actual) == [(Literal("A"),)]
