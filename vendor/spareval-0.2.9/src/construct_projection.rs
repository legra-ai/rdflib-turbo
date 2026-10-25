//! Typed output domains over the shared CONSTRUCT evaluator and identity cache.
use std::borrow::Cow;

use oxrdf::{GeneralizedTriple, Term, Triple};

pub(crate) trait ConstructProjection: 'static {
    type Output: 'static;
    const PROPAGATE_EXTERNALIZATION_ERRORS: bool;
    fn project(subject: Term, predicate: Term, object: Term) -> Option<Self::Output>;
    fn uncached(output: &Self::Output) -> bool;
    fn terms(output: &Self::Output) -> [Cow<'_, Term>; 3];
}

pub(crate) struct StrictConstruct;
impl ConstructProjection for StrictConstruct {
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
    fn terms(output: &Triple) -> [Cow<'_, Term>; 3] {
        [
            Cow::Owned(output.subject.clone().into()),
            Cow::Owned(output.predicate.clone().into()),
            Cow::Borrowed(&output.object),
        ]
    }
}

pub(crate) struct InferenceConstruct;
impl ConstructProjection for InferenceConstruct {
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
    fn terms(output: &GeneralizedTriple) -> [Cow<'_, Term>; 3] {
        [
            Cow::Borrowed(&output.subject),
            Cow::Borrowed(&output.predicate),
            Cow::Borrowed(&output.object),
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
