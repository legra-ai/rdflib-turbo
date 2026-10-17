use oxigraph::model::{GraphName, Literal, NamedNode, Quad};
use oxigraph::store::Store;

#[test]
fn exact_literals_survive_reopen_and_uncommitted_changes_do_not() {
    let directory = tempfile::tempdir().unwrap();
    let datatype = NamedNode::new("http://www.w3.org/2001/XMLSchema#integer").unwrap();
    let quad = |value| {
        Quad::new(
            NamedNode::new("urn:s").unwrap(),
            NamedNode::new("urn:p").unwrap(),
            Literal::new_typed_literal(value, datatype.clone()),
            GraphName::DefaultGraph,
        )
    };
    {
        let store = Store::open(directory.path()).unwrap();
        let mut transaction = store.start_transaction().unwrap();
        transaction.insert(&quad("01"));
        transaction.insert(&quad("1"));
        assert_eq!(transaction.len().unwrap(), 2);
        assert_eq!(store.len().unwrap(), 0);
        transaction.commit().unwrap();
        let mut transaction = store.start_transaction().unwrap();
        transaction.remove(&quad("01"));
        transaction.insert(&quad("2"));
        assert_eq!(transaction.len().unwrap(), 2);
        // Dropping the transaction must abort both operations.
    }
    let store = Store::open(directory.path()).unwrap();
    assert_eq!(store.len().unwrap(), 2);
    assert!(store.contains(&quad("01")).unwrap());
    assert!(store.contains(&quad("1")).unwrap());
    assert!(!store.contains(&quad("2")).unwrap());
}
