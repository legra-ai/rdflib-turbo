//! Isolated feature matrix for the patched storage dependency.

#[cfg(test)]
mod cardinality_tests;
#[cfg(test)]
mod lexical_projection_tests;
#[cfg(test)]
mod projection_tests;

#[cfg(all(test, feature = "exact"))]
#[path = "../../../src/store_tests.rs"]
mod store_tests;

#[cfg(all(test, feature = "persistence", feature = "exact"))]
mod persistence_tests;

#[cfg(all(test, not(feature = "exact")))]
mod upstream_tests {
    use oxigraph::model::{GraphName, Literal, NamedNode, Quad, Term};
    use oxigraph::store::Store;

    #[test]
    fn upstream_canonicalization_is_unchanged_without_the_opt_in() {
        let store = Store::new().unwrap();
        let datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap();
        let subject = NamedNode::new("urn:s").unwrap();
        let predicate = NamedNode::new("urn:p").unwrap();
        for spelling in ["01", "1"] {
            store
                .insert(&Quad::new(
                    subject.clone(),
                    predicate.clone(),
                    Literal::new_typed_literal(spelling, datatype.clone()),
                    GraphName::DefaultGraph,
                ))
                .unwrap();
        }
        assert_eq!(store.len().unwrap(), 1);
        assert_eq!(
            store.iter().next().unwrap().unwrap().object,
            Term::Literal(Literal::new_typed_literal("1", datatype))
        );
    }
}

#[cfg(all(test, feature = "rdf-12", feature = "exact"))]
mod rdf12_tests {
    use oxigraph::model::{BaseDirection, GraphName, Literal, NamedNode, Quad, Triple};
    use oxigraph::store::Store;

    #[test]
    fn triple_terms_and_directional_languages_preserve_identity() {
        let store = Store::new().unwrap();
        let node = || NamedNode::new("urn:node").unwrap();
        let numeric = Literal::new_typed_literal(
            "01",
            NamedNode::new("http://www.w3.org/2001/XMLSchema#byte").unwrap(),
        );
        let embedded = Quad::new(
            node(),
            node(),
            Triple::new(node(), node(), numeric),
            GraphName::DefaultGraph,
        );
        let directional = Quad::new(
            node(),
            node(),
            Literal::new_directional_language_tagged_literal("hello", "en", BaseDirection::Ltr)
                .unwrap(),
            GraphName::DefaultGraph,
        );
        for quad in [&embedded, &directional] {
            store.insert(quad).unwrap();
            assert!(store.contains(quad).unwrap());
            assert!(store.iter().any(|entry| entry.unwrap() == *quad));
        }
        let mut transaction = store.start_transaction().unwrap();
        transaction.remove(&embedded);
        assert_eq!(transaction.len().unwrap(), 1);
        drop(transaction);
        assert_eq!(store.len().unwrap(), 2);
    }
}
