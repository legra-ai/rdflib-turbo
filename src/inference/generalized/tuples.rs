//! Indexed tuple access for specialized kernels over one immutable round.

use oxigraph::model::*;

use super::Generalized;
use crate::Result;
use crate::inference::Pending;
use crate::inference::backend::Graph;

pub(in crate::inference) struct Tuples<'a, G> {
    pub(in crate::inference) data: &'a G,
    pub(in crate::inference) extra: &'a Generalized,
}

/// A logical inference tuple; subject and predicate need not be RDF-representable.
pub(in crate::inference) struct Tuple {
    pub(in crate::inference) subject: Term,
    pub(in crate::inference) object: Term,
}

impl<G: Graph> Tuples<'_, G> {
    fn subject(term: TermRef<'_>) -> Option<NamedOrBlankNodeRef<'_>> {
        match term {
            TermRef::NamedNode(node) => Some(node.into()),
            TermRef::BlankNode(node) => Some(node.into()),
            TermRef::Literal(_) => None,
        }
    }

    fn predicate(term: TermRef<'_>) -> Option<NamedNodeRef<'_>> {
        match term {
            TermRef::NamedNode(node) => Some(node),
            _ => None,
        }
    }

    pub(in crate::inference) fn matching<'a>(
        &'a self,
        pattern: [Option<TermRef<'_>>; 3],
    ) -> impl Iterator<Item = Result<[Term; 3]>> + use<'a, G> {
        let [subject, predicate, object] = pattern;
        let standard_subject = subject.and_then(Self::subject);
        let standard_predicate = predicate.and_then(Self::predicate);
        let ordinary = ((subject.is_none() || standard_subject.is_some())
            && (predicate.is_none() || standard_predicate.is_some()))
        .then(|| {
            self.data
                .quads(standard_subject, standard_predicate, object)
        })
        .into_iter()
        .flatten()
        .map(|quad| {
            let quad = quad?;
            Ok([quad.subject.into(), quad.predicate.into(), quad.object])
        });
        ordinary.chain(
            self.extra
                .matching(pattern)
                .map(|row| Ok(row.map(TermRef::into_owned))),
        )
    }

    pub(in crate::inference) fn quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<NamedNodeRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<Tuple>> + use<'a, G> {
        self.matching([subject, predicate.map(TermRef::from), object])
            .map(|row| {
                let [subject, _, object] = row?;
                Ok(Tuple { subject, object })
            })
    }

    pub(in crate::inference) fn contains(&self, row: &[Term; 3]) -> Result<bool> {
        match (
            Self::subject(row[0].as_ref()),
            Self::predicate(row[1].as_ref()),
        ) {
            (Some(subject), Some(predicate)) => self.data.contains(&Quad::new(
                subject,
                predicate,
                row[2].clone(),
                GraphName::DefaultGraph,
            )),
            _ => Ok(self.extra.contains(row)),
        }
    }

    pub(in crate::inference) fn push(
        &self,
        row: [Term; 3],
        ordinary: &mut Pending,
        generalized: &mut Generalized,
    ) -> Result<()> {
        self.extra.retain_row(row, self.data, ordinary, generalized)
    }
}
