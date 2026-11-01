//! Value-slot observations for predicate traversal; base RDF terms remain unchanged.
use crate::{
    BooleanObservation, BuiltinBooleanState, GraphRead, Result, SelectedSlot, SelectedValueFrame,
};
use oxigraph::model::*;
use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::sync::Arc;

/// A count consumer cannot accidentally acquire or inspect RDF words.
pub(crate) enum ValueCollection {
    Words {
        terms: Vec<Term>,
        boolean: Vec<(usize, BooleanObservation)>,
    },
    Cardinality(usize),
}

/// Ordinary values or a checked count-only source projection.
pub(crate) struct ObservedValues {
    collection: ValueCollection,
    #[cfg(test)]
    lookup_steps: std::cell::Cell<usize>,
}

// Production count operands reuse the existing two-vector record's niche;
// lookup instrumentation is deliberately excluded from this layout proof.
#[cfg(not(test))]
const _: () = assert!(
    std::mem::size_of::<ObservedValues>()
        == std::mem::size_of::<(Vec<Term>, Vec<(usize, BooleanObservation)>)>()
);
impl ObservedValues {
    fn words_with(terms: Vec<Term>, boolean: Vec<(usize, BooleanObservation)>) -> Self {
        Self {
            collection: ValueCollection::Words { terms, boolean },
            #[cfg(test)]
            lookup_steps: std::cell::Cell::new(0),
        }
    }
    pub fn cardinality(count: usize) -> Self {
        Self {
            collection: ValueCollection::Cardinality(count),
            #[cfg(test)]
            lookup_steps: std::cell::Cell::new(0),
        }
    }
    fn parts(&self) -> (&[Term], &[(usize, BooleanObservation)]) {
        match &self.collection {
            ValueCollection::Words { terms, boolean } => (terms, boolean),
            ValueCollection::Cardinality(_) => {
                panic!("Count-only operand reached an RDF word consumer")
            }
        }
    }
    fn words(&self) -> &[Term] {
        self.parts().0
    }
    pub fn len(&self) -> usize {
        match &self.collection {
            ValueCollection::Words { terms, .. } => terms.len(),
            ValueCollection::Cardinality(count) => *count,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn lexical(terms: Vec<Term>) -> Self {
        Self::words_with(terms, Vec::new())
    }
    pub fn focus(term: Term, observation: Option<crate::BooleanObservation>) -> Self {
        Self::words_with(
            vec![term],
            observation.map(|o| vec![(0, o)]).unwrap_or_default(),
        )
    }
    pub fn predicate(
        data: &(impl GraphRead + ?Sized),
        focus: &Term,
        predicate: &NamedNode,
    ) -> Result<Self> {
        let mut seen = HashSet::new();
        // bounded: this one SP projection has at most two canonical Boolean RDF words.
        let mut observations = HashMap::new();
        for row in data.matching_quads(
            Some(focus.as_ref()),
            Some(predicate.as_ref().into()),
            None,
            Some(GraphNameRef::DefaultGraph),
        ) {
            let row = row?;
            let state = data.boolean_object_state(&row)?;
            if seen.insert(row.object.clone())
                && let Some(state) = state
                && let Term::Literal(literal) = &row.object
                && BuiltinBooleanState::lexical_default(literal.value()) != Some(state)
            {
                observations.insert(row.object, state);
            }
        }
        let mut terms: Vec<_> = seen.into_iter().collect();
        terms.sort_by_key(ToString::to_string);
        let boolean = terms
            .iter()
            .enumerate()
            .filter_map(|(i, t)| {
                observations
                    .get(t)
                    .map(|s| (i, BooleanObservation::Attested(*s)))
            })
            .collect();
        Ok(Self::words_with(terms, boolean))
    }
    pub fn selected(
        frame: &Arc<dyn SelectedValueFrame>,
        focus: &Term,
        predicate: &NamedNode,
        subject: Option<&BooleanObservation>,
    ) -> Result<Self> {
        let selected = frame.predicate(
            focus,
            predicate,
            subject.and_then(BooleanObservation::subject),
        )?;
        Self::from_selected(frame, selected)
    }
    pub fn selected_sequence(
        frame: &Arc<dyn SelectedValueFrame>,
        focus: &Term,
        path: crate::SelectedPathRef<'_>,
        subject: Option<&BooleanObservation>,
    ) -> Result<Self> {
        let selected =
            frame.sequence(focus, path, subject.and_then(BooleanObservation::subject))?;
        Self::from_selected(frame, selected)
    }
    fn from_selected(
        frame: &Arc<dyn SelectedValueFrame>,
        selected: Vec<crate::SelectedTerm>,
    ) -> Result<Self> {
        // Each selected row contributes at most one optional observation.
        let first_observation_capacity = selected.len().min(4);
        let mut terms = Vec::with_capacity(selected.len());
        let mut boolean = Vec::new();
        for row in selected {
            if row.slot.is_none()
                && matches!(&row.term,Term::Literal(literal)
                if literal.datatype()==vocab::xsd::BOOLEAN
                && BuiltinBooleanState::lexical_default(literal.value()).is_some())
            {
                return Err(crate::Error::Invalid(
                    "Selected Boolean value has no issued owner slot".into(),
                ));
            }
            let index = terms.len();
            terms.push(row.term);
            if let Some(slot) = row.slot {
                if boolean.is_empty() {
                    boolean.reserve_exact(first_observation_capacity);
                }
                boolean.push((
                    index,
                    BooleanObservation::Selected(SelectedSlot::new(frame, slot)),
                ));
            }
        }
        // The source already acquired the actual Python set. No RDF-key
        // deduplication or display ordering may replace that owner's sequence.
        Ok(Self::words_with(terms, boolean))
    }
    pub fn slots(&self) -> impl ExactSizeIterator<Item = ValueSlot<'_>> {
        (0..self.words().len()).map(|index| ValueSlot { owner: self, index })
    }
    fn observation_at(&self, index: usize) -> Option<&BooleanObservation> {
        // Every producer appends metadata in strictly increasing value-slot order.
        let entry = self
            .parts()
            .1
            .binary_search_by_key(&index, |(stored, _)| {
                #[cfg(test)]
                self.lookup_steps.set(self.lookup_steps.get() + 1);
                *stored
            })
            .ok()?;
        Some(&self.parts().1[entry].1)
    }
}

/// A borrowed slot can only be issued by its owning active values projection.
#[derive(Clone, Copy)]
pub(crate) struct ValueSlot<'a> {
    owner: &'a ObservedValues,
    index: usize,
}
impl<'a> ValueSlot<'a> {
    pub fn term(self) -> &'a Term {
        &self.owner.words()[self.index]
    }
    pub fn observation(self) -> Option<&'a BooleanObservation> {
        self.owner.observation_at(self.index)
    }
    pub fn datatype(self, expected: &NamedNode) -> Result<bool> {
        match self.observation() {
            Some(BooleanObservation::Selected(slot)) => slot.datatype(expected),
            Some(BooleanObservation::Focus(slot))
                if matches!(self.term(), Term::Literal(l)
                if l.datatype().as_str() == "http://www.w3.org/2001/XMLSchema#boolean"
                    && BuiltinBooleanState::lexical_default(l.value()).is_some()) =>
            {
                slot.datatype(expected)
            }
            Some(BooleanObservation::Attested(state)) if state.assessment() == Some(true) => {
                Ok(false)
            }
            Some(BooleanObservation::Reported(_)) => Err(crate::Error::Invalid(
                "Reported owner is not an active value slot".into(),
            )),
            _ => Ok(crate::value::datatype(self.term(), expected)),
        }
    }
}

#[cfg(test)]
#[path = "observed_values_tests.rs"]
mod tests;

impl Deref for ObservedValues {
    type Target = [Term];
    fn deref(&self) -> &Self::Target {
        self.words()
    }
}
impl<'a> IntoIterator for &'a ObservedValues {
    type Item = &'a Term;
    type IntoIter = std::slice::Iter<'a, Term>;
    fn into_iter(self) -> Self::IntoIter {
        self.words().iter()
    }
}
