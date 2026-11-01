use super::*;
use crate::{ReportTermRole, SelectedLiteral, SelectedTerm};
use std::any::Any;

#[derive(Debug)]
struct Frame(usize, bool);
impl SelectedValueFrame for Frame {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn begin_nested(&self) -> crate::Result<Arc<dyn SelectedValueFrame>> {
        panic!("This test issuer has no nested shape acquisition")
    }
    fn predicate(
        &self,
        _: &Term,
        _: &NamedNode,
        _: Option<crate::SelectedSubjectRef<'_>>,
    ) -> Result<Vec<SelectedTerm>> {
        Ok((0..self.0)
            .map(|slot| SelectedTerm {
                term: Literal::from(false).into(),
                slot: self.1.then_some(slot),
            })
            .collect())
    }
    fn sequence(
        &self,
        _: &Term,
        _: crate::SelectedPathRef<'_>,
        _: Option<crate::SelectedSubjectRef<'_>>,
    ) -> crate::Result<Vec<crate::SelectedTerm>> {
        panic!("This predicate-only test issuer does not acquire sequences")
    }
    fn datatype(&self, slot: usize, _: &NamedNode) -> Result<bool> {
        Ok(slot % 2 == 1)
    }
    fn retain(&self, _: usize) -> Result<SelectedLiteral> {
        panic!("not a report consumer")
    }
    fn present(&self, _: usize, _: ReportTermRole) -> Result<String> {
        panic!("not a report consumer")
    }
    fn close(&self) {}
}
fn selected(count: usize) -> (ObservedValues, Arc<dyn SelectedValueFrame>) {
    let frame: Arc<dyn SelectedValueFrame> = Arc::new(Frame(count, true));
    let values = ObservedValues::selected(
        &frame,
        &NamedNode::new("urn:s").unwrap().into(),
        &NamedNode::new("urn:p").unwrap(),
        None,
    )
    .unwrap();
    (values, frame)
}
fn boolean() -> NamedNode {
    NamedNode::new("http://www.w3.org/2001/XMLSchema#boolean").unwrap()
}

#[test]
fn cloned_words_require_explicit_inherited_slot_authority() {
    let (values, _frame) = selected(2);
    let mut slots = values.slots();
    let first = slots.next().unwrap();
    let second = slots.next().unwrap();
    assert_eq!(first.term(), second.term());
    assert!(!first.datatype(&boolean()).unwrap());
    assert!(second.datatype(&boolean()).unwrap());
    let cloned = first.term().clone();
    assert!(
        !values
            .slots()
            .any(|slot| std::ptr::eq(slot.term(), &cloned))
    );
    let inherited = ObservedValues::focus(cloned, first.observation().cloned());
    assert!(
        !inherited
            .slots()
            .next()
            .unwrap()
            .datatype(&boolean())
            .unwrap()
    );
}
#[test]
fn high_owner_count_lookup_work_is_logarithmic_per_value() {
    let n = 1024;
    let (values, _frame) = selected(n);
    for (index, slot) in values.slots().enumerate() {
        assert_eq!(slot.datatype(&boolean()).unwrap(), index % 2 == 1);
    }
    let steps = values.lookup_steps.get();
    println!("VALUE_LOOKUP highB n={n} steps={steps}");
    assert!(steps <= n * (n.ilog2() as usize + 2), "{steps}");
}
#[test]
fn lexical_m0_datatype_does_not_scan_other_value_slots() {
    let n = 1024;
    let values = ObservedValues::lexical(
        (0..n)
            .map(|i| NamedNode::new(format!("urn:{i}")).unwrap().into())
            .collect(),
    );
    for slot in values.slots() {
        assert!(!slot.datatype(&boolean()).unwrap());
    }
    let steps = values.lookup_steps.get();
    println!("VALUE_LOOKUP M0 n={n} steps={steps}");
    assert_eq!(steps, 0);
}
#[test]
fn selected_canonical_boolean_cannot_omit_its_owner_slot() {
    let frame: Arc<dyn SelectedValueFrame> = Arc::new(Frame(1, false));
    let result = ObservedValues::selected(
        &frame,
        &NamedNode::new("urn:s").unwrap().into(),
        &NamedNode::new("urn:p").unwrap(),
        None,
    );
    assert!(
        matches!(result,Err(crate::Error::Invalid(message)) if message=="Selected Boolean value has no issued owner slot")
    );
}
#[test]
fn value_slot_and_test_counter_layouts_are_explicit() {
    println!(
        "VALUE_SLOT_LAYOUT slot={} values_cfg_test={} owned_test_counter={}",
        std::mem::size_of::<ValueSlot<'_>>(),
        std::mem::size_of::<ObservedValues>(),
        std::mem::size_of::<std::cell::Cell<usize>>()
    );
    assert_eq!(
        std::mem::size_of::<ValueSlot<'_>>(),
        2 * std::mem::size_of::<usize>()
    );
}

#[test]
fn selected_path_ref_borrows_only_flat_named_sequence_steps() {
    use crate::path::Path;
    let p = NamedNode::new("urn:p").unwrap();
    let q = NamedNode::new("urn:q").unwrap();
    let path = Path::Sequence(vec![
        Path::Predicate(p.clone()),
        Path::Predicate(q.clone()),
        Path::Predicate(p.clone()),
    ]);
    let view = crate::SelectedPathRef::predicate_sequence(&path).unwrap();
    assert_eq!(std::mem::size_of_val(&view), 16);
    assert_eq!(view.predicates().collect::<Vec<_>>(), vec![&p, &q, &p]);
    assert!(crate::SelectedPathRef::predicate_sequence(&Path::Predicate(p.clone())).is_none());
    assert!(
        crate::SelectedPathRef::predicate_sequence(&Path::Sequence(vec![Path::Predicate(
            p.clone()
        )]))
        .is_none()
    );
    assert!(
        crate::SelectedPathRef::predicate_sequence(&Path::Sequence(vec![
            Path::Predicate(p),
            Path::Inverse(Box::new(Path::Predicate(q)))
        ]))
        .is_none()
    );
}
