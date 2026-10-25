//! Typed output domains over the shared CONSTRUCT evaluator and identity cache.
use crate::eval::EvalDataset;
use crate::{InferenceTerm, QueryEvaluationError, QueryableDataset};

use oxrdf::{GeneralizedTriple, Term, TermRef, Triple};

pub(crate) trait ConstructProjection<'a, D: QueryableDataset<'a>>: 'static {
    type Output: 'static;
    const PROPAGATE_EXTERNALIZATION_ERRORS: bool;
    fn project(subject: Term, predicate: Term, object: Term) -> Option<Self::Output>;
    fn uncached(output: &Self::Output) -> bool;
    fn terms(output: &Self::Output) -> [TermRef<'_>; 3];
    fn project_internal(
        &mut self,
        terms: &[InferenceTerm<'_, D::InternalTerm>; 3],
        dataset: &EvalDataset<'a, D>,
    ) -> Result<Option<Self::Output>, QueryEvaluationError> {
        let externalize = |term: &InferenceTerm<'_, D::InternalTerm>| {
            if Self::PROPAGATE_EXTERNALIZATION_ERRORS {
                term.externalize_checked(dataset).map(Some)
            } else {
                Ok(term.externalize(dataset))
            }
        };
        let subject = externalize(&terms[0])?;
        let predicate = externalize(&terms[1])?;
        let object = externalize(&terms[2])?;
        let (Some(subject), Some(predicate), Some(object)) = (subject, predicate, object) else {
            return Ok(None);
        };
        Ok(Self::project(subject, predicate, object))
    }
    fn clear(&mut self) {}
}

pub(crate) struct StrictConstruct;
impl<'a, D: QueryableDataset<'a>> ConstructProjection<'a, D> for StrictConstruct {
    type Output = Triple;
    const PROPAGATE_EXTERNALIZATION_ERRORS: bool = false;
    fn project(subject: Term, predicate: Term, object: Term) -> Option<Triple> {
        Some(Triple {
            subject: subject.try_into().ok()?,
            predicate: predicate.try_into().ok()?,
            object,
        })
    }
    fn uncached(output: &Triple) -> bool {
        output.subject.is_blank_node() || uncached_term(&output.object)
    }
    fn terms(output: &Triple) -> [TermRef<'_>; 3] {
        [
            output.subject.as_ref().into(),
            output.predicate.as_ref().into(),
            output.object.as_ref(),
        ]
    }
}

pub(crate) struct InferenceConstruct;
impl<'a, D: QueryableDataset<'a>> ConstructProjection<'a, D> for InferenceConstruct {
    type Output = GeneralizedTriple;
    const PROPAGATE_EXTERNALIZATION_ERRORS: bool = true;
    fn project(subject: Term, predicate: Term, object: Term) -> Option<GeneralizedTriple> {
        Some(GeneralizedTriple {
            subject,
            predicate,
            object,
        })
    }
    fn uncached(output: &GeneralizedTriple) -> bool {
        uncached_term(&output.subject) || uncached_term(&output.object)
    }
    fn terms(output: &GeneralizedTriple) -> [TermRef<'_>; 3] {
        [
            output.subject.as_ref(),
            output.predicate.as_ref(),
            output.object.as_ref(),
        ]
    }
}

fn uncached_term(term: &Term) -> bool {
    if term.is_blank_node() {
        return true;
    }
    #[cfg(feature = "sparql-12")]
    if term.is_triple() {
        return true;
    }
    false
}

/// A query-local inference projection retaining native term identities until projection.
/// IDs belong only to `dataset`; implementations must never compare IDs across owners.
pub trait InferenceProjection<'a, D: QueryableDataset<'a>>: 'static {
    /// An independently owned head, allowed to outlive its query and dictionary reader.
    type Output: 'static;
    /// Projects terms in subject/predicate/object order. Errors terminate the head stream.
    fn project(
        &mut self,
        terms: &[InferenceTerm<'_, D::InternalTerm>; 3],
        dataset: &D,
    ) -> Result<Self::Output, QueryEvaluationError>;
    /// Borrows full canonical term identities, including explicit datatype and language.
    fn terms(output: &Self::Output) -> [TermRef<'_>; 3];
    /// Releases query-local projection caches when the existing result cache is retired.
    fn clear(&mut self);
}

pub(crate) struct ProjectedInference<P>(pub P);
impl<'a, D: QueryableDataset<'a>, P: InferenceProjection<'a, D>> ConstructProjection<'a, D>
    for ProjectedInference<P>
{
    type Output = P::Output;
    const PROPAGATE_EXTERNALIZATION_ERRORS: bool = true;
    fn project(_: Term, _: Term, _: Term) -> Option<Self::Output> {
        unreachable!("native heads use internal projection")
    }
    fn uncached(output: &Self::Output) -> bool {
        let [subject, _, object] = P::terms(output);
        uncached_ref(subject) || uncached_ref(object)
    }
    fn terms(output: &Self::Output) -> [TermRef<'_>; 3] {
        P::terms(output)
    }
    fn project_internal(
        &mut self,
        terms: &[InferenceTerm<'_, D::InternalTerm>; 3],
        dataset: &EvalDataset<'a, D>,
    ) -> Result<Option<Self::Output>, QueryEvaluationError> {
        self.0.project(terms, &dataset.dataset).map(Some)
    }
    fn clear(&mut self) {
        self.0.clear();
    }
}
fn uncached_ref(term: TermRef<'_>) -> bool {
    if term.is_blank_node() {
        return true;
    }
    #[cfg(feature = "sparql-12")]
    if term.is_triple() {
        return true;
    }
    false
}
