//! Term readers and lexical predicates shared by the diagnostic checks.

use oxigraph::model::*;

use super::super::backend::Graph;
use super::super::diagnostic_datatypes::{self, LexicalContext, LexicalResult};
use crate::{Error, Result};

pub(super) fn text(term: &Term) -> &str {
    match term {
        Term::NamedNode(node) => node.as_str(),
        Term::BlankNode(node) => node.as_str(),
        Term::Literal(literal) => literal.value(),
    }
}

pub(super) fn has(
    data: &impl Graph,
    left: &Term,
    predicate: TermRef<'_>,
    right: &Term,
) -> Result<bool> {
    data.contains(GeneralizedQuadRef {
        subject: left.as_ref(),
        predicate,
        object: right.as_ref(),
        graph_name: GraphNameRef::DefaultGraph,
    })
}

pub(super) fn objects<'a>(
    data: &'a impl Graph,
    node: &'a Term,
    predicate: &'a NamedNode,
) -> impl Iterator<Item = Result<Term>> + 'a {
    data.quads(Some(node.as_ref()), Some(predicate.as_ref().into()), None)
        .map(|quad| quad.map(|quad| quad.object))
}

pub(in crate::inference) fn valid_lexical(
    literal: &Literal,
    context: &LexicalContext,
) -> Result<bool> {
    match diagnostic_datatypes::check(literal, context) {
        LexicalResult::Valid => Ok(true),
        LexicalResult::Diagnostic => Ok(false),
        LexicalResult::Failure(failure) => Err(Error::Lexical(failure)),
    }
}

pub(in crate::inference) fn cardinality_zero(
    term: &Term,
    context: &LexicalContext,
) -> Result<bool> {
    match term {
        Term::Literal(literal) => {
            diagnostic_datatypes::cardinality_zero(literal, context).map_err(Error::Lexical)
        }
        Term::NamedNode(_) => Err(Error::Attribute(
            "'URIRef' object has no attribute 'value'".into(),
        )),
        Term::BlankNode(_) => Err(Error::Attribute(
            "'BNode' object has no attribute 'value'".into(),
        )),
    }
}
