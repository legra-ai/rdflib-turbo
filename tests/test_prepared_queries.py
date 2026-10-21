"""Prepared RDFLib algebra executes natively without replaying source text."""

import pytest
from rdflib import BNode, Literal, URIRef, Variable
from rdflib import Graph as ReferenceGraph
from rdflib.compare import isomorphic
from rdflib.plugins.sparql import prepareQuery
from rdflib.plugins.sparql.algebra import translateQuery
from rdflib.plugins.sparql.parser import parseQuery
from rdflib_turbo import Graph, StoreDataset

DATA = '<urn:a> <urn:p> "A"; <urn:q> "yes". <urn:b> <urn:p> "B".'
QUERIES = [
    "SELECT ?s WHERE {?s <urn:p> ?o}",
    "ASK {?s <urn:p> ?o}",
    "CONSTRUCT {?s <urn:q> ?o} WHERE {?s <urn:p> ?o}",
    "DESCRIBE <urn:a>",
    'SELECT ?s ?v WHERE {?s <urn:p> ?o OPTIONAL {?s <urn:q> ?v FILTER(?v="no")}}',
    "SELECT ?s WHERE {?s <urn:p> ?o FILTER EXISTS {?s <urn:q> ?v}}",
    "SELECT ?s WHERE {?s <urn:p> ?o FILTER NOT EXISTS {?s <urn:q> ?v}}",
    "SELECT ?s WHERE {?s <urn:p> ?o} ORDER BY ?s OFFSET 1",
    "SELECT ?s WHERE {{SELECT DISTINCT ?s WHERE {?s <urn:p> ?o} ORDER BY ?s LIMIT 1}}",
    'SELECT ?v WHERE {VALUES ?v {"01"^^<http://www.w3.org/2001/XMLSchema#integer>}}',
]


def equal(actual, expected):
    assert actual.type == expected.type
    if actual.type == "ASK":
        assert bool(actual) == bool(expected)
    elif actual.type == "SELECT":
        assert actual.vars == expected.vars
        assert sorted(actual.bindings, key=repr) == sorted(expected.bindings, key=repr)
    else:
        assert isomorphic(actual.graph, expected.graph)


@pytest.mark.parametrize("source", QUERIES)
@pytest.mark.parametrize("factory", [Graph, StoreDataset])
def test_prepared_query_forms_and_algebra_match_reference(source, factory):
    expected_graph = ReferenceGraph().parse(data=DATA, format="turtle")
    actual_graph = factory().parse(data=DATA, format="turtle")
    expected = expected_graph.query(prepareQuery(source))
    prepared = prepareQuery(source)
    equal(actual_graph.query(prepared), expected)
    equal(actual_graph.query(prepared), expected_graph.query(prepareQuery(source)))


def test_prepared_namespace_snapshot_ignores_mutated_original_args():
    namespaces = {"ex": URIRef("urn:old:")}
    prepared = prepareQuery("SELECT ?o WHERE {?s ex:p ?o}", initNs=namespaces)
    namespaces["ex"] = URIRef("urn:new:")
    graph = Graph()
    graph.add((URIRef("urn:s"), URIRef("urn:old:p"), Literal("old")))
    graph.add((URIRef("urn:s"), URIRef("urn:new:p"), Literal("new")))
    assert list(graph.query(prepared, initNs={"ex": URIRef("urn:new:")})) == [(Literal("old"),)]


def test_prepared_without_original_args_and_supplied_lazy_metadata():
    source = "SELECT ?s WHERE {VALUES ?s {<urn:a>} {SELECT ?s WHERE {BIND(<urn:b> AS ?s)}}}"
    prepared = translateQuery(parseQuery(source))
    assert not hasattr(prepared, "_original_args")
    assert list(Graph().query(prepared, initBindings={"unused": URIRef("urn:x")})) == [
        (URIRef("urn:a"),)
    ]
    prepared.algebra.p.p.lazy = False
    assert list(Graph().query(prepared, initBindings={"unused": URIRef("urn:x")})) == []


def test_prepared_exists_keeps_caller_expression_and_scope():
    source = "SELECT ?s WHERE {?s <urn:p> ?o FILTER EXISTS {{BIND(<urn:b> AS ?s)} OPTIONAL {?s <urn:q> ?v}}}"
    prepared = prepareQuery(source)
    expression = prepared.algebra.p.p.expr
    before = repr(prepared.algebra)
    graph = Graph().parse(data=DATA, format="turtle")
    expected = ReferenceGraph().parse(data=DATA, format="turtle").query(prepareQuery(source))
    equal(graph.query(prepared), expected)
    assert prepared.algebra.p.p.expr is expression
    assert repr(prepared.algebra) == before


def test_prepared_slice_length_uses_actual_attribute_override():
    prepared = prepareQuery("SELECT ?s WHERE {?s <urn:p> ?o} ORDER BY ?s LIMIT 1")
    prepared.algebra.p.length = 2
    assert prepared.algebra.p["length"] == 1
    expected = ReferenceGraph().parse(data=DATA, format="turtle").query(prepared)
    equal(Graph().parse(data=DATA, format="turtle").query(prepared), expected)


def test_prepared_child_uses_actual_attribute_override():
    prepared = prepareQuery("SELECT ?o WHERE {?s <urn:p> ?o}")
    replacement = prepareQuery("SELECT ?o WHERE {?s <urn:q> ?o}")
    prepared.algebra.p.p = replacement.algebra.p.p
    expected = ReferenceGraph().parse(data=DATA, format="turtle").query(prepared)
    equal(Graph().parse(data=DATA, format="turtle").query(prepared), expected)


def test_prepared_dataset_clause_uses_actual_attribute_override():
    from rdflib import Dataset as ReferenceDataset

    prepared = prepareQuery("SELECT ?o WHERE {?s <urn:p> ?o}")
    other = prepareQuery("SELECT ?o FROM <urn:selected> WHERE {?s <urn:p> ?o}")
    prepared.algebra.datasetClause = other.algebra.datasetClause
    expected = ReferenceDataset()
    actual = StoreDataset()
    for graph in (expected, actual):
        graph.default_graph.add((URIRef("urn:s"), URIRef("urn:p"), Literal("default")))
        graph.graph(URIRef("urn:selected")).add(
            (URIRef("urn:s"), URIRef("urn:p"), Literal("named"))
        )
    equal(actual.query(prepared), expected.query(prepared))


@pytest.mark.parametrize(
    "source",
    [
        "SELECT ?o WHERE {[] <urn:p> ?o}",
        "SELECT ?o WHERE {_:query <urn:p> ?o}",
        "SELECT ?o WHERE {?focus <urn:p> ?o}",
    ],
)
def test_prepared_blank_patterns_and_blank_initial_terms(source):
    from rdflib import BNode

    focus = BNode("held")
    expected = ReferenceGraph()
    actual = Graph()
    for graph in (expected, actual):
        graph.add((focus, URIRef("urn:p"), Literal("blank")))
        graph.add((URIRef("urn:other"), URIRef("urn:p"), Literal("iri")))
    equal(
        actual.query(prepareQuery(source), initBindings={"focus": focus}),
        expected.query(prepareQuery(source), initBindings={"focus": focus}),
    )


def test_prepared_exists_accepts_already_translated_reference_state():
    source = "SELECT ?s WHERE {?s <urn:p> ?o FILTER EXISTS {?s <urn:q> ?v}}"
    prepared = prepareQuery(source)
    expected = ReferenceGraph().parse(data=DATA, format="turtle").query(prepared)
    list(expected)
    before = repr(prepared.algebra)
    equal(Graph().parse(data=DATA, format="turtle").query(prepared), expected)
    assert repr(prepared.algebra) == before


def test_prepared_optional_metadata_field_must_be_present():
    import json

    from rdflib_turbo._prepared import snapshot

    prepared = prepareQuery("SELECT ?s WHERE {?s <urn:p> ?o OPTIONAL {?s <urn:q> ?v}}")
    transport = json.loads(snapshot(prepared))
    del transport["pattern"]["inner"]["variables"]
    graph = Graph()
    with pytest.raises(ValueError, match="missing field"):
        graph.store.backend._native.query_prepared(json.dumps(transport))


def test_prepared_graph_ignores_dataset_clauses_like_reference_graph():
    prepared = prepareQuery("SELECT ?o FROM <urn:unused> WHERE {?s <urn:p> ?o}")
    expected = ReferenceGraph().parse(data=DATA, format="turtle").query(prepared)
    equal(Graph().parse(data=DATA, format="turtle").query(prepared), expected)


def test_prepared_describe_generated_variables_do_not_replace_user_bindings():
    prepared = prepareQuery("DESCRIBE <urn:a> ?__describe_0 WHERE {BIND(<urn:b> AS ?__describe_0)}")
    prepared.algebra.PV = [URIRef("urn:a"), Variable("__describe_0")]
    expected = ReferenceGraph().parse(data=DATA, format="turtle").query(prepared)
    equal(Graph().parse(data=DATA, format="turtle").query(prepared), expected)


def raw_exists(prepared):
    from collections import OrderedDict

    expression = prepared.algebra.p.p.expr
    expression.graph = OrderedDict.__getitem__(expression, "graph")
    assert expression.graph.name == "GroupGraphPatternSub"
    return prepared


@pytest.mark.parametrize(
    "clause",
    [
        "GROUP BY ?s HAVING(COUNT(?o)>10)",
        "ORDER BY DESC(?s)",
    ],
)
def test_prepared_raw_subselect_never_silently_drops_unimplemented_clauses(clause):
    source = f"SELECT ?s WHERE {{?s <urn:p> ?o FILTER EXISTS {{{{SELECT ?s WHERE {{?s <urn:p> ?o}} {clause}}}}}}}"
    # The reference accepts these clauses. Until the native raw-syntax adapter
    # implements them, explicit failure is required rather than wrong success.
    list(ReferenceGraph().parse(data=DATA, format="turtle").query(prepareQuery(source)))
    with pytest.raises(NotImplementedError, match="prepared"):
        Graph().parse(data=DATA, format="turtle").query(raw_exists(prepareQuery(source)))


def test_prepared_raw_subselect_never_silently_discards_projection_expression():
    source = (
        "SELECT ?s WHERE {?s <urn:p> ?o FILTER EXISTS {{SELECT (1 AS ?x) WHERE {?s <urn:p> ?o}}}}"
    )
    list(ReferenceGraph().parse(data=DATA, format="turtle").query(prepareQuery(source)))
    with pytest.raises(NotImplementedError, match="prepared"):
        Graph().parse(data=DATA, format="turtle").query(raw_exists(prepareQuery(source)))


def test_prepared_raw_exists_merges_adjacent_blank_node_bgp_blocks_across_filters():
    source = 'ASK {FILTER EXISTS {_:focus <urn:p> "A" FILTER(true) _:focus <urn:q> "yes"}}'
    expected = ReferenceGraph()
    actual = Graph()
    for graph in (expected, actual):
        graph.add((URIRef("urn:a"), URIRef("urn:p"), Literal("A")))
        graph.add((URIRef("urn:b"), URIRef("urn:q"), Literal("yes")))
    assert not bool(expected.query(prepareQuery(source)))
    equal(actual.query(prepareQuery(source)), expected.query(prepareQuery(source)))


@pytest.mark.parametrize("scope", [None, set()])
def test_prepared_optional_preserves_missing_versus_empty_source_scope(scope):
    source = 'SELECT ?x WHERE {{BIND(<urn:a> AS ?s) BIND(1 AS ?x)} OPTIONAL {?s <urn:p> "B"}}'
    prepared = prepareQuery(source)
    prepared.algebra.p.p.p1._vars = scope
    expected = ReferenceGraph().parse(data=DATA, format="turtle").query(prepared)
    equal(Graph().parse(data=DATA, format="turtle").query(prepared), expected)


@pytest.mark.parametrize(
    "source,bindings",
    [
        ("SELECT ?s WHERE {BIND(<urn:a> AS ?s) FILTER EXISTS {{FILTER(?s=<urn:a>)}}}", {}),
        ("SELECT ?s WHERE {BIND(<urn:a> AS ?s) FILTER EXISTS {FILTER(?s=<urn:a>)}}", {}),
        (
            "SELECT ?s WHERE {BIND(<urn:a> AS ?s) FILTER EXISTS {{BIND(<urn:a> AS ?v) FILTER(?v=<urn:a>)}}}",
            {},
        ),
        (
            "SELECT ?s WHERE {FILTER EXISTS {{BIND(<urn:b> AS ?s) FILTER(?s=<urn:b>)}}}",
            {"s": URIRef("urn:a")},
        ),
        (
            "SELECT ?s WHERE {BIND(<urn:a> AS ?s) FILTER EXISTS {{SELECT ?v WHERE {FILTER(?s=<urn:a>)}}}}",
            {},
        ),
    ],
)
def test_prepared_filter_scope_preserves_inherited_new_and_initial_bindings(source, bindings):
    expected = ReferenceGraph().query(prepareQuery(source), initBindings=bindings)
    equal(Graph().query(prepareQuery(source), initBindings=bindings), expected)


def test_prepared_filter_keeps_supplied_scope_and_isolation_attributes():
    prepared = prepareQuery(
        "SELECT ?s WHERE {BIND(<urn:a> AS ?s) FILTER EXISTS {{FILTER(?s=<urn:a>)}}}"
    )
    guard = prepared.algebra.p.p.expr.graph.p2
    assert guard.name == "Filter"
    guard._vars = {Variable("s")}
    guard.no_isolated_scope = False
    expected = ReferenceGraph().query(prepared)
    equal(Graph().query(prepared), expected)


BLANK_DATA = '<urn:a> <urn:p> "A" . <urn:b> <urn:q> "yes" .'


@pytest.mark.parametrize(
    "source",
    [
        'ASK {_:b <urn:p> "A" {_:b <urn:q> "yes"}}',
        'ASK {_:b <urn:p> "A" {_:c <urn:q> "yes"}}',
        'ASK {_:b <urn:p> "A" FILTER EXISTS {_:b <urn:q> "yes"}}',
        'ASK {_:b <urn:p> "A" FILTER NOT EXISTS {_:b <urn:q> "yes"}}',
        'SELECT ?x WHERE {_:b <urn:p> "A" {SELECT ?x WHERE {_:b <urn:q> "yes" BIND(1 AS ?x)}}}',
        'SELECT ?x WHERE {_:b <urn:p> "A" {SELECT DISTINCT ?x WHERE {_:b <urn:q> "yes" BIND(1 AS ?x)}}}',
        'SELECT ?x WHERE {_:b <urn:p> "A" {SELECT ?x WHERE {_:b <urn:q> "yes" BIND(1 AS ?x)} LIMIT 1}}',
        'SELECT ?x WHERE {{SELECT ?x WHERE {_:b <urn:p> "A" BIND(1 AS ?x)}} _:b <urn:q> "yes"}',
        'SELECT ?__prepared_blank_0 WHERE {BIND(<urn:user> AS ?__prepared_blank_0) _:b <urn:p> "A"}',
        "CONSTRUCT {_:b <urn:copy> ?v} WHERE {_:b <urn:p> ?v}",
        'CONSTRUCT {?__prepared_blank_0 <urn:copy> "x"} WHERE {_:b <urn:p> "A"}',
    ],
)
def test_prepared_blank_identity_follows_actual_query_contexts(source):
    expected = ReferenceGraph().parse(data=BLANK_DATA, format="turtle").query(prepareQuery(source))
    equal(Graph().parse(data=BLANK_DATA, format="turtle").query(prepareQuery(source)), expected)


def test_prepared_blank_fresh_names_avoid_unused_initial_parameter_names():
    source = 'ASK {_:b <urn:p> "A"}'
    bindings = {"__prepared_blank_0": URIRef("urn:unrelated")}
    expected = (
        ReferenceGraph()
        .parse(data=BLANK_DATA, format="turtle")
        .query(prepareQuery(source), initBindings=bindings)
    )
    assert bool(expected)
    equal(
        Graph()
        .parse(data=BLANK_DATA, format="turtle")
        .query(prepareQuery(source), initBindings=bindings),
        expected,
    )


@pytest.mark.parametrize("key", [Variable("b"), BNode("b"), "b"])
@pytest.mark.parametrize("query_blank", [False, True])
def test_prepared_optional_source_scope_preserves_binding_key_types(key, query_blank):
    left = '_:b <urn:p> "A"' if query_blank else "BIND(<urn:a> AS ?b)"
    right = '_:b <urn:q> "yes"' if query_blank else '?b <urn:q> "yes"'
    prepared = prepareQuery(f"SELECT ?x WHERE {{{left} BIND(1 AS ?x) OPTIONAL {{{right}}}}}")
    prepared.algebra.p.p.p1._vars = {Variable("x"), key}
    expected = ReferenceGraph().parse(data=BLANK_DATA, format="turtle").query(prepared)
    equal(Graph().parse(data=BLANK_DATA, format="turtle").query(prepared), expected)


@pytest.mark.parametrize("key", [Variable("s"), BNode("s"), "s"])
def test_prepared_filter_source_scope_preserves_binding_key_types(key):
    prepared = prepareQuery(
        "SELECT ?s WHERE {BIND(<urn:a> AS ?s) FILTER EXISTS {{FILTER(?s=<urn:a>)}}}"
    )
    prepared.algebra.p.p.expr.graph.p2._vars = {key}
    expected = ReferenceGraph().query(prepared)
    equal(Graph().query(prepared), expected)
