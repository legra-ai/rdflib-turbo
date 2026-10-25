//! Shared template materialization, retaining checked errors for inference heads.
use super::*;

pub(super) fn internal<'a, 't, D: QueryableDataset<'a>>(
    selector: &'t TripleTemplateValue,
    tuple: &InternalTuple<D::InternalTerm>,
    bnodes: &mut Vec<BlankNode>,
    dataset: &EvalDataset<'a, D>,
) -> Result<Option<ConstructTerm<'t, D::InternalTerm>>, QueryEvaluationError> {
    match selector {
        TripleTemplateValue::Constant(term) => Ok(Some(ConstructTerm::Constant(term))),
        TripleTemplateValue::Variable(variable) => {
            Ok(tuple.get(*variable).cloned().map(ConstructTerm::Internal))
        }
        _ => external(selector, tuple, bnodes, dataset)
            .map(|value| value.map(ConstructTerm::External)),
    }
}

pub(super) fn external<'a, D: QueryableDataset<'a>>(
    selector: &TripleTemplateValue,
    tuple: &InternalTuple<D::InternalTerm>,
    bnodes: &mut Vec<BlankNode>,
    dataset: &EvalDataset<'a, D>,
) -> Result<Option<Term>, QueryEvaluationError> {
    match selector {
        TripleTemplateValue::Constant(term) => Ok(Some(term.clone())),
        TripleTemplateValue::Variable(variable) => tuple
            .get(*variable)
            .map(|term| dataset.externalize_term(term.clone()))
            .transpose(),
        TripleTemplateValue::BlankNode(label) => {
            if *label >= bnodes.len() {
                bnodes.resize_with(*label + 1, BlankNode::default);
            }
            Ok(Some(bnodes[*label].clone().into()))
        }
        #[cfg(feature = "sparql-12")]
        TripleTemplateValue::Triple(triple) => {
            let Some(subject) = external(&triple.subject, tuple, bnodes, dataset)? else {
                return Ok(None);
            };
            let subject: NamedOrBlankNode = subject.try_into().map_err(|_| {
                QueryEvaluationError::Unexpected(Box::new(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Inference nested RDF triple subject must be an IRI or blank node",
                )))
            })?;
            let Some(predicate) = external(&triple.predicate, tuple, bnodes, dataset)? else {
                return Ok(None);
            };
            let predicate: NamedNode = predicate.try_into().map_err(|_| {
                QueryEvaluationError::Unexpected(Box::new(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Inference nested RDF triple predicate must be an IRI",
                )))
            })?;
            let Some(object) = external(&triple.object, tuple, bnodes, dataset)? else {
                return Ok(None);
            };
            Ok(Some(
                Triple {
                    subject,
                    predicate,
                    object,
                }
                .into(),
            ))
        }
    }
}
