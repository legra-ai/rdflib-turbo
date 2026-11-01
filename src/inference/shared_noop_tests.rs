//! Reflexive subproperty joins must not materialize already-visible heads.
use super::*;
#[cfg(feature = "allocation-profiling")]
use crate::inference::HeadDisposition;
use crate::inference::{Graph, Pending, backend::TransactionGraph};
use oxigraph::{
    model::{BlankNode, GeneralizedQuad, GraphName, Literal as RdfLiteral},
    store::Store,
};

const ROWS: usize = 1024;

fn iri(word: &str) -> NamedNode {
    NamedNode::new(word).unwrap()
}

fn fixture(nonself: bool) -> Store {
    let store = Store::new().unwrap();
    store.insert_named_graph(&iri("urn:destination")).unwrap();
    for target in ["urn:p"].into_iter().chain(nonself.then_some("urn:q")) {
        store
            .insert_generalized(&GeneralizedQuad::new(
                iri("urn:p"),
                iri("http://www.w3.org/2000/01/rdf-schema#subPropertyOf"),
                iri(target),
                BlankNode::new("schema").unwrap(),
            ))
            .unwrap();
    }
    for index in 0..ROWS {
        store
            .insert_generalized(&GeneralizedQuad::new(
                iri(&format!("urn:subject:{index}")),
                iri("urn:p"),
                RdfLiteral::new_simple_literal(format!("payload {index}")),
                iri("urn:source"),
            ))
            .unwrap();
    }
    store
}

fn snapshot(store: &Store) -> std::collections::HashSet<GeneralizedQuad> {
    // bounded: the fixed 1024 fixture rows, two schema edges and 1024 derived heads.
    store
        .generalized_quads_for_pattern(None, None, None, None)
        .map(|row| row.unwrap())
        .collect()
}

#[test]
fn catalogue_reflexive_subproperty_has_no_output_rows() {
    let store = fixture(false);
    let before = snapshot(&store);
    let mut transaction = store.start_transaction().unwrap();
    let graph = TransactionGraph {
        transaction: &mut transaction,
        destination: iri("urn:destination").into(),
    };
    let original = CompiledRule::try_new(&Rdfs7.definition())
        .unwrap()
        .into_query();
    assert_eq!(
        graph
            .projected_inference_heads(original)
            .unwrap()
            .try_fold(0, |count, row| row.map(|_| count + 1))
            .unwrap(),
        ROWS
    );
    let optimized = CompiledRule::from_catalogue(Rdfs7).unwrap().into_query();
    assert_eq!(
        graph
            .projected_inference_heads(optimized)
            .unwrap()
            .try_fold(0, |count, row| row.map(|_| count + 1))
            .unwrap(),
        0,
        "A reflexive subproperty join only reproduces its existing body fact"
    );
    drop(transaction);
    assert_eq!(snapshot(&store), before);
}

#[test]
fn nonself_subproperty_heads_preserve_source_scope_and_rollback() {
    let store = fixture(true);
    let before = snapshot(&store);
    let mut transaction = store.start_transaction().unwrap();
    let mut graph = TransactionGraph {
        transaction: &mut transaction,
        destination: iri("urn:destination").into(),
    };
    let mut pending = Pending::default();
    let query = CompiledRule::from_catalogue(Rdfs7).unwrap().into_query();
    for head in graph.projected_inference_heads(query).unwrap() {
        pending.push_head(&graph, head.unwrap()).unwrap();
    }
    assert_eq!(pending.len(), ROWS);
    for head in pending.into_heads() {
        assert_eq!(head.as_ref().predicate, iri("urn:q").as_ref().into());
        graph.insert_borrowed(head.as_ref()).unwrap();
    }
    // bounded: exactly the 1024 admitted heads in the declared destination.
    let rows = graph
        .transaction
        .generalized_quads_for_pattern(
            None,
            None,
            None,
            Some(iri("urn:destination").as_ref().into()),
        )
        .collect::<std::result::Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(rows.len(), ROWS);
    assert!(rows.iter().all(|row| {
        row.predicate == iri("urn:q").into()
            && row.graph_name == GraphName::NamedNode(iri("urn:destination"))
    }));
    drop(transaction);
    assert_eq!(snapshot(&store), before);
    assert!(store.contains_named_graph(&iri("urn:destination")).unwrap());
}

#[test]
fn unexpected_subproperty_catalogue_shapes_are_rejected() {
    for defect in ["head", "schema", "shared-variable", "body"] {
        let mut rule = Rdfs7.definition();
        match defect {
            "head" => rule.head.terms[0] = rule.head.terms[2].clone(),
            "schema" => {
                let Literal::Positive(schema) = &mut rule.body[0] else {
                    unreachable!();
                };
                schema.terms[1] = rule.head.terms[1].clone();
            }
            "shared-variable" => {
                let Literal::Positive(assertion) = &mut rule.body[1] else {
                    unreachable!();
                };
                assertion.terms[1] = assertion.terms[0].clone();
            }
            "body" => rule.body.push(rule.body[0].clone()),
            _ => unreachable!(),
        }
        assert!(matches!(
            CompiledRule::without_self_subproperty(&rule),
            Err(Error::Unsupported(message))
                if message == "Unexpected subproperty catalogue shape"
        ));
    }
}

#[cfg(feature = "allocation-profiling")]
#[test]
#[ignore = "global allocation counters require isolated execution"]
fn reflexive_subproperty_allocation_budget() {
    let store = fixture(false);
    let before = snapshot(&store);
    let query = CompiledRule::from_catalogue(Rdfs7).unwrap().into_query();
    let mut transaction = store.start_transaction().unwrap();
    let graph = TransactionGraph {
        transaction: &mut transaction,
        destination: iri("urn:destination").into(),
    };
    let (_, allocations) = crate::allocation_tests::measure_requested(|| {
        let mut pending = Pending::default();
        for row in graph.projected_inference_heads(query.clone()).unwrap() {
            assert_eq!(
                pending.push_head(&graph, row.unwrap()).unwrap(),
                HeadDisposition::AlreadyPresent
            );
        }
        assert!(pending.is_empty());
    });
    drop(transaction);
    assert_eq!(snapshot(&store), before);
    println!(
        "REFLEXIVE_SUBPROPERTY_ALLOCATIONS {}",
        serde_json::to_string(&allocations).unwrap()
    );
    assert!(
        allocations.allocated < 512 * 1024,
        "A no-op subproperty join allocated {} bytes for 1024 unchanged rows",
        allocations.allocated
    );
}
