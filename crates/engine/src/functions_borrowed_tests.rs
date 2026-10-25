//! Query callbacks must borrow the pending transaction, including nested functions.
use super::*;
use oxigraph::store::Store;

#[test]
fn nested_functions_read_pending_transaction_and_preserve_exact_terms() {
    let shapes = crate::parse_graph(
        r#"@prefix sh: <http://www.w3.org/ns/shacl#> .
        <urn:inner> a sh:SPARQLFunction;
          sh:select "SELECT ?result WHERE { <urn:s> <urn:p> ?result }" .
        <urn:outer> a sh:SPARQLFunction;
          sh:select "SELECT (<urn:inner>() AS ?result) WHERE {}" ."#,
        oxigraph::io::RdfFormat::Turtle,
        None,
    )
    .unwrap();
    let store = Store::new().unwrap();
    let literal = Literal::new_typed_literal("007", NamedNode::new_unchecked("urn:datatype"));
    let mut transaction = store.start_transaction().unwrap();
    {
        transaction.insert(
            Quad::new(
                NamedNode::new_unchecked("urn:s"),
                NamedNode::new_unchecked("urn:p"),
                literal.clone(),
                GraphName::DefaultGraph,
            )
            .as_ref(),
        );
        let QueryResults::Solutions(mut rows) = evaluate_query(
            &transaction,
            &shapes,
            "SELECT (<urn:outer>() AS ?value) WHERE {}",
            &[],
        )
        .unwrap() else {
            panic!("SELECT required")
        };
        assert_eq!(
            rows.next().unwrap().unwrap().get("value"),
            Some(&Term::from(literal.clone()))
        );
        assert!(rows.next().is_none());
    }
    drop(transaction);
    assert_eq!(store.len().unwrap(), 0);
}
