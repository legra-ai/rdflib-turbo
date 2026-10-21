//! FILTER source metadata and optimizer entry contexts survive native planning.
use crate::Optimizer;
use crate::algebra::{Expression, GraphPattern, JoinAlgorithm};
use oxrdf::{Literal, Variable};
use spargebra::algebra::GraphPattern as SourcePattern;
use spargebra::{Query, SparqlParser};

#[test]
fn scoped_filter_is_an_optimizer_boundary_even_below_a_join() {
    let variable = Variable::new_unchecked("scope");
    let filter = GraphPattern::RdfLibFilter {
        inner: Box::new(GraphPattern::Extend {
            inner: Box::new(GraphPattern::empty_singleton()),
            variable: variable.clone(),
            expression: Expression::Literal(Literal::from(1)),
        }),
        expression: Expression::Equal(
            Box::new(Expression::Variable(variable.clone())),
            Box::new(Expression::Literal(Literal::from(1))),
        ),
        variables: None,
        no_isolated_scope: false,
    };
    let plan = GraphPattern::Join {
        left: Box::new(GraphPattern::empty_singleton()),
        right: Box::new(filter.clone()),
        algorithm: JoinAlgorithm::RdfLibIndependent,
    };
    assert_eq!(Optimizer::optimize_graph_pattern(plan.clone()), plan);
    let mut variables = Vec::new();
    filter.lookup_used_variables(&mut |variable| variables.push(variable.clone()));
    assert!(variables.contains(&variable));
    assert!(matches!(
        SourcePattern::from(&filter),
        SourcePattern::Filter { .. }
    ));
}

#[test]
fn scoped_filter_preserves_entry_join_but_disjoint_branch_is_normalized() {
    let filter = GraphPattern::RdfLibFilter {
        inner: Box::new(GraphPattern::empty_singleton()),
        expression: Expression::Bound(Variable::new_unchecked("inherited")),
        variables: None,
        no_isolated_scope: false,
    };
    // Removing/reordering this join changes which bindings are inherited when
    // the filter evaluates its expression. Its independent sibling has no such
    // entry-context dependency and must still receive normal optimization.
    let sibling = GraphPattern::Filter {
        inner: Box::new(GraphPattern::empty_singleton()),
        expression: Expression::Literal(Literal::from(true)),
    };
    let plan = GraphPattern::Join {
        left: Box::new(sibling),
        right: Box::new(filter.clone()),
        algorithm: JoinAlgorithm::RdfLibLazy,
    };
    let GraphPattern::Join {
        left,
        right,
        algorithm,
    } = Optimizer::optimize_graph_pattern(plan)
    else {
        panic!("context-bearing join must remain");
    };
    assert_eq!(*left, GraphPattern::empty_singleton());
    assert_eq!(*right, filter);
    assert_eq!(algorithm, JoinAlgorithm::RdfLibLazy);
}

#[test]
fn original_filter_scope_differs_from_translated_exists_scope_without_changing_sparql() {
    let Query::Select { pattern, .. } = SparqlParser::new()
        .with_rdflib_query_context()
        .parse_query("SELECT ?s WHERE {BIND(<urn:a> AS ?s) FILTER EXISTS {{FILTER(?s=<urn:a>)}}}")
        .unwrap()
    else {
        panic!("SELECT");
    };
    let GraphPattern::Project { inner, .. } = GraphPattern::from_rdflib_algebra(&pattern) else {
        panic!("projection");
    };
    let GraphPattern::RdfLibFilter {
        variables,
        expression: Expression::Exists(body),
        no_isolated_scope,
        ..
    } = *inner
    else {
        panic!("outer scoped filter");
    };
    assert!(
        variables
            .unwrap()
            .iter()
            .any(|variable| variable.as_str() == "s")
    );
    assert!(!no_isolated_scope);
    let GraphPattern::Join { right, .. } = *body else {
        panic!("grouped EXISTS join");
    };
    let GraphPattern::RdfLibFilter {
        variables,
        no_isolated_scope,
        ..
    } = *right
    else {
        panic!("inner scoped filter");
    };
    assert!(variables.is_none());
    assert!(!no_isolated_scope);
    let GraphPattern::Project { inner, .. } = GraphPattern::from(&pattern) else {
        panic!("projection");
    };
    assert!(matches!(*inner, GraphPattern::Filter { .. }));
}
