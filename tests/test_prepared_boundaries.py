"""Prepared algebra has explicit native support and atomic rejection boundaries."""

import pytest
from rdflib import Dataset as ReferenceDataset
from rdflib import Graph as ReferenceGraph
from rdflib import Literal, URIRef
from rdflib.plugins.sparql import prepareQuery
from rdflib.plugins.sparql.parserutils import CompValue
from rdflib_turbo import Graph, StoreDataset

DATA = '<urn:a> <urn:p> 1; <urn:q> "yes" . <urn:b> <urn:p> 2 .'


def bindings(graph, source):
    return sorted(graph.query(prepareQuery(source)).bindings, key=repr)


@pytest.mark.parametrize(
    "source",
    [
        "SELECT ?v WHERE {VALUES ?v {0 1 2} FILTER((?v=1 && ?v<2) || ?v=2)}",
        "SELECT ?v WHERE {VALUES ?v {0 1 2} FILTER(!(?v=0) && -?v < +2)}",
        'SELECT ?s WHERE {?s <urn:p> ?v FILTER EXISTS {?s <urn:p> ?n OPTIONAL {?s <urn:q> ?q FILTER(?q="yes")} MINUS {?s <urn:q> "never"}}}',
        'SELECT ?s WHERE {?s <urn:p> ?v FILTER EXISTS {{?s <urn:q> "yes"} UNION {?s <urn:p> 2}}}',
        "SELECT ?s WHERE {?s <urn:p> ?v FILTER EXISTS {{SELECT REDUCED ?s WHERE {?s <urn:p> ?v} LIMIT 1}}}",
        "SELECT ?s WHERE {?s <urn:p> ?v FILTER EXISTS {{SELECT ?s WHERE {?s <urn:p> ?v} OFFSET 1}}}",
        "SELECT ?s WHERE {?s <urn:p> ?v FILTER EXISTS {{SELECT ?s WHERE {?s <urn:p> ?v} LIMIT 1 OFFSET 1}}}",
    ],
)
@pytest.mark.parametrize("factory", [Graph, StoreDataset])
def test_supported_prepared_expression_and_exists_boundaries_match_reference(source, factory):
    reference = ReferenceGraph().parse(data=DATA, format="turtle")
    actual = factory().parse(data=DATA, format="turtle")
    assert bindings(actual, source) == bindings(reference, source)


@pytest.mark.parametrize(
    "body,message",
    [
        ("{SELECT ?s WHERE {?s <urn:p> ?v} GROUP BY ?s}", "clause: groupby"),
        ("{SELECT ?s WHERE {?s <urn:p> ?v} HAVING(COUNT(?v)>0)}", "clause: having"),
        ("{SELECT ?s WHERE {?s <urn:p> ?v} ORDER BY ?s}", "clause: orderby"),
        ("{SELECT ?s WHERE {?s <urn:p> ?v} VALUES ?s {<urn:a>}}", "clause: valuesClause"),
        ("{SELECT (?s AS ?alias) WHERE {?s <urn:p> ?v}}", "projection expression"),
        ("VALUES ?x {1}", "EXISTS syntax: InlineData"),
        ("GRAPH <urn:g> {?s <urn:p> ?v}", "EXISTS syntax: GraphGraphPattern"),
    ],
)
def test_unsupported_raw_exists_is_rejected_before_mutation(body, message):
    source = f"SELECT ?ok WHERE {{BIND(EXISTS {{{body}}} AS ?ok)}}"
    prepared = prepareQuery(source)
    # RDFLib public Query objects may retain the authentic parser graph.
    prepared.algebra.p.p.expr.graph = prepared.algebra.p.p.expr["graph"]
    graph = Graph().parse(data=DATA, format="turtle")
    before = set(graph)
    with pytest.raises(NotImplementedError, match=message):
        graph.query(prepared)
    assert set(graph) == before
    assert bool(graph.query("ASK {?s <urn:p> ?v}"))


@pytest.mark.parametrize(
    "source,message",
    [
        ('SELECT ?s WHERE {?s <urn:p> ?v FILTER(STR(?s)="urn:a")}', "expression: Builtin_STR"),
        ("SELECT ?n WHERE {?s <urn:p> ?v BIND(?v+1 AS ?n)}", "expression: AdditiveExpression"),
        (
            "SELECT ?s WHERE {SERVICE <urn:never-contact> {?s <urn:p> ?v}}",
            "graph algebra: ServiceGraphPattern",
        ),
    ],
)
def test_unsupported_prepared_operators_fail_without_fallback(source, message):
    graph = Graph().parse(data=DATA, format="turtle")
    with pytest.raises(NotImplementedError, match=message):
        graph.query(prepareQuery(source))
    assert len(graph) == 3


def test_prepared_named_graph_uses_dataset_context():
    reference, actual = ReferenceDataset(), StoreDataset()
    for dataset in (reference, actual):
        dataset.graph(URIRef("urn:g")).parse(data=DATA, format="turtle")
    source = "SELECT ?s WHERE {GRAPH <urn:g> {?s <urn:p> 1}}"
    assert bindings(actual, source) == bindings(reference, source)


def test_unknown_prepared_form_fails_explicitly():
    prepared = prepareQuery("ASK {}")
    prepared.algebra = CompValue("UnknownQuery", p=prepared.algebra.p)
    with pytest.raises(NotImplementedError, match="query form: UnknownQuery"):
        Graph().query(prepared)


def test_prepared_unknown_options_abort_the_active_transaction():
    dataset = StoreDataset()
    with pytest.raises(RuntimeError, match=r"aborted|closed|active"):
        with dataset.transaction() as transaction:
            transaction.default_graph.add((URIRef("urn:pending"), URIRef("urn:p"), Literal(1)))
            with pytest.raises(TypeError, match="Unsupported native query options"):
                transaction.default_graph.query(prepareQuery("ASK {}"), unsupported=True)
    assert not list(dataset)


@pytest.mark.parametrize(
    "body",
    [
        "{SELECT * WHERE {?s <urn:p> ?v}}",
        "{SELECT DISTINCT ?s WHERE {?s <urn:p> ?v} LIMIT 1}",
        "{SELECT REDUCED ?s WHERE {?s <urn:p> ?v} OFFSET 1}",
        "{SELECT ?s WHERE {?s <urn:p> ?v} LIMIT 1 OFFSET 1}",
        "?s <urn:p> ?v FILTER(?v=1)",
        "BIND(1 AS ?v) FILTER(?v=1)",
        "{?s <urn:p> 1} UNION {?s <urn:p> 2}",
        "?s <urn:p> ?v OPTIONAL {?s <urn:q> ?q}",
        '?s <urn:p> ?v OPTIONAL {?s <urn:q> ?q FILTER(?q="yes")}',
        '?s <urn:p> ?v MINUS {?s <urn:q> "yes"}',
    ],
)
def test_supplied_raw_exists_graph_matches_reference(body):
    source = f"SELECT ?ok WHERE {{BIND(EXISTS {{{body}}} AS ?ok)}}"
    actual_prepared, reference_prepared = prepareQuery(source), prepareQuery(source)
    # The snapshot accepts the retained parser form; reference executes its
    # normal translated form of the exact same source.
    actual_prepared.algebra.p.p.expr.graph = actual_prepared.algebra.p.p.expr["graph"]
    reference = ReferenceGraph().parse(data=DATA, format="turtle")
    actual = Graph().parse(data=DATA, format="turtle")
    assert actual.query(actual_prepared).bindings == reference.query(reference_prepared).bindings


def test_optional_scope_literal_key_stays_distinct_from_variable_names():
    source = "SELECT ?s WHERE {?s <urn:p> ?v OPTIONAL {?s <urn:q> ?q}}"
    actual_prepared, reference_prepared = prepareQuery(source), prepareQuery(source)
    for prepared in (actual_prepared, reference_prepared):
        prepared.algebra.p.p.p1._vars = {Literal("s")}
    reference = ReferenceGraph().parse(data=DATA, format="turtle")
    actual = Graph().parse(data=DATA, format="turtle")
    assert sorted(actual.query(actual_prepared).bindings, key=repr) == sorted(
        reference.query(reference_prepared).bindings, key=repr
    )


@pytest.mark.parametrize("optional", [False, True])
def test_supplied_parser_filter_keeps_its_expression_guard(optional):
    # RDFLib normally removes Filter syntax nodes during translation. Its public
    # CompValue/Query objects can retain them; reuse an actual translated guard
    # and compare the same source's normal reference execution.
    body = "?s <urn:p> ?v FILTER(?v=1)"
    if optional:
        body = '?s <urn:p> ?v OPTIONAL {?s <urn:q> ?q FILTER(?q="yes")}'
    source = f"SELECT ?ok WHERE {{BIND(EXISTS {{{body}}} AS ?ok)}}"
    prepared = prepareQuery(source)
    exists = prepared.algebra.p.p.expr
    translated = exists.graph
    raw = exists["graph"]
    if optional:
        optional_syntax = next(part for part in raw.part if part.name == "OptionalGraphPattern")
        optional_syntax.graph.part.append(CompValue("Filter", expr=translated.expr))
    else:
        raw.part.append(CompValue("Filter", expr=translated.expr))
    exists.graph = raw
    actual = Graph().parse(data=DATA, format="turtle")
    reference = ReferenceGraph().parse(data=DATA, format="turtle")
    assert actual.query(prepared).bindings == reference.query(prepareQuery(source)).bindings


def test_prepared_snapshot_boundary_rejects_non_query_objects():
    from rdflib_turbo._prepared import snapshot

    with pytest.raises(TypeError, match="A prepared RDFLib Query is required"):
        snapshot("ASK {}")
