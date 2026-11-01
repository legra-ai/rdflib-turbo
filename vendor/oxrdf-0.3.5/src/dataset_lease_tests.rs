use super::*;
use crate::{GraphName, Literal, NamedNode, Quad};
use std::collections::hash_map::DefaultHasher;

fn quad() -> Quad {
    Quad::new(
        NamedNode::new("urn:s").unwrap(),
        NamedNode::new("urn:p").unwrap(),
        Literal::from(false),
        GraphName::DefaultGraph,
    )
}
#[test]
fn foreign_cloned_owner_fails_before_metadata_map_insertion() {
    let q = quad();
    let mut builder = DatasetLeaseBuilder::new();
    builder.insert(q.as_ref());
    let raw = builder.quad_lease(q.as_ref()).unwrap();
    let owner = builder.freeze();
    // bounded: one-quad fixture clone deliberately shares the existing record Arc.
    let foreign = DatasetLeaseBuilder::from_dataset(owner.dataset().clone()).freeze();
    assert!(Arc::ptr_eq(
        &raw.record,
        &foreign.record(q.as_ref()).unwrap()
    ));
    assert_eq!(
        foreign.bind_lease(raw.clone()).unwrap_err(),
        DatasetLeaseError::ForeignOwner
    );
    let valid = owner.bind_lease(raw).unwrap();
    assert!(owner.validate_lease(&valid).is_ok());
    assert_eq!(
        foreign.validate_lease(&valid),
        Err(DatasetLeaseError::ForeignOwner)
    );
    let mut entries = std::collections::HashMap::new();
    if let Ok(lease) = foreign.bind_lease(InsertionQuadLease {
        record: Arc::clone(&valid.record),
        origin: valid.origin.clone(),
    }) {
        entries.insert(lease, 1);
    }
    assert!(entries.is_empty());
}
#[test]
fn key_uses_full_record_equality_not_record_pointer() {
    let q = quad();
    let mut b = DatasetLeaseBuilder::new();
    b.insert(q.as_ref());
    let raw = b.quad_lease(q.as_ref()).unwrap();
    let owner = b.freeze();
    let a = owner.bind_lease(raw).unwrap();
    let equal = DatasetQuadLease {
        record: Arc::new((*a.record).clone()),
        origin: a.origin.clone(),
    };
    assert!(!Arc::ptr_eq(&a.record, &equal.record));
    assert_eq!(a, equal);
    let mut ah = DefaultHasher::new();
    let mut bh = DefaultHasher::new();
    a.hash(&mut ah);
    equal.hash(&mut bh);
    assert_eq!(ah.finish(), bh.finish());
    assert!(owner.lease_quad(quad().as_ref()).is_some());
    println!(
        "DATASET_LEASE_LAYOUT raw={} checked={} frozen={} base={}",
        size_of::<InsertionQuadLease>(),
        size_of::<DatasetQuadLease>(),
        size_of::<FrozenDataset>(),
        size_of::<Dataset>()
    );
}
