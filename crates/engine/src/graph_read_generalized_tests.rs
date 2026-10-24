use oxigraph::model::*;
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use std::collections::HashSet;

use crate::{GraphRead, QueryGraph};

fn fixture() -> GeneralizedDataset {
    let literal = Literal::new_simple_literal("literal");
    let named = NamedNode::new_unchecked("urn:same");
    let blank = BlankNode::new_unchecked("urn:same");
    let mut data = GeneralizedDataset::new();
    for graph in [
        GraphName::DefaultGraph,
        named.clone().into(),
        blank.clone().into(),
    ] {
        for row in [
            GeneralizedQuad::new(literal.clone(), blank.clone(), named.clone(), graph.clone()),
            GeneralizedQuad::new(
                blank.clone(),
                literal.clone(),
                literal.clone(),
                graph.clone(),
            ),
        ] {
            data.insert(row.as_ref());
        }
    }
    data
}

#[test]
fn generalized_graph_read_retains_full_quad_roles_and_pattern_scope() {
    let data = fixture();
    for target in data.iter() {
        for mask in 0..16 {
            let s = (mask & 1 != 0).then_some(target.subject);
            let p = (mask & 2 != 0).then_some(target.predicate);
            let o = (mask & 4 != 0).then_some(target.object);
            let g = (mask & 8 != 0).then_some(target.graph_name);
            let wanted: HashSet<_> = data
                .iter()
                .filter(|q| {
                    s.is_none_or(|s| q.subject == s)
                        && p.is_none_or(|p| q.predicate == p)
                        && o.is_none_or(|o| q.object == o)
                        && g.is_none_or(|g| q.graph_name == g)
                })
                .map(GeneralizedQuadRef::into_owned)
                .collect();
            let actual: HashSet<_> = data
                .matching_quads(s, p, o, g)
                .map(Result::unwrap)
                .collect();
            assert_eq!(actual, wanted, "mask={mask}");
        }
    }
}

#[test]
fn generalized_graph_metadata_streams_without_strict_quad_narrowing() {
    let data = fixture();
    let graphs = || {
        GraphRead::named_graphs(&data)
            .map(Result::unwrap)
            .collect::<Vec<_>>()
    };
    let names = graphs();
    assert_eq!(names, graphs());
    assert_eq!(names.len(), 2);
    assert_ne!(names[0], names[1]);
    for name in names {
        assert!(data.contains_named_graph(&name).unwrap());
    }
    assert!(
        !data
            .contains_named_graph(&NamedNode::new_unchecked("urn:missing").into())
            .unwrap()
    );
}

#[test]
fn generalized_graph_query_preserves_literal_predicates_and_named_scope() {
    let data = fixture();
    for query in [
        "ASK { ?s ?p ?o FILTER(isLiteral(?p)) }",
        "ASK { GRAPH <urn:same> { ?s ?p ?o FILTER(isLiteral(?p)) } }",
        "ASK { GRAPH ?g { ?s ?p ?o FILTER(isBlank(?g)) } }",
    ] {
        let result = SparqlEvaluator::new()
            .parse_query(query)
            .unwrap()
            .on_queryable_dataset(QueryGraph::new(&data))
            .execute()
            .unwrap();
        assert!(matches!(result, QueryResults::Boolean(true)));
    }
}
