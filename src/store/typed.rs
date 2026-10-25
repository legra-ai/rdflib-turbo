//! One typed RDF boundary shared by graph scans and mutations.
use oxigraph::model::{
    BlankNode, GeneralizedQuad, GraphName, Literal, NamedNode, NamedOrBlankNode, Term,
};

use super::typed_view::{Kind, Position, TermView};
use crate::Result;

pub(super) type TermTuple = (String, String, Option<String>, Option<String>);

pub(super) fn term(source: TermTuple) -> Result<Term> {
    term_in_position(source, Position::Data)
}

fn term_in_position(
    (kind, value, datatype, language): TermTuple,
    position: Position,
) -> Result<Term> {
    let validated = TermView::new(&kind, &value, datatype.as_deref(), language.as_deref())
        .validate(position)?;
    Ok(match validated {
        Kind::Iri => NamedNode::new_unchecked(value).into(),
        Kind::Blank => BlankNode::new_unchecked(value).into(),
        Kind::Literal => if let Some(mut language) = language {
            language.make_ascii_lowercase();
            Literal::new_language_tagged_literal_unchecked(value, language)
        } else if let Some(datatype) = datatype {
            Literal::new_typed_literal(value, NamedNode::new_unchecked(datatype))
        } else {
            Literal::new_simple_literal(value)
        }
        .into(),
    })
}

pub(super) fn graph_name(source: TermTuple) -> Result<NamedOrBlankNode> {
    match term_in_position(source, Position::GraphName)? {
        Term::NamedNode(node) => Ok(node.into()),
        Term::BlankNode(node) => Ok(node.into()),
        _ => unreachable!("graph name kind was validated before model conversion"),
    }
}

pub(super) fn graph(source: Option<TermTuple>) -> Result<GraphName> {
    Ok(match source.map(graph_name).transpose()? {
        None => GraphName::DefaultGraph,
        Some(NamedOrBlankNode::NamedNode(node)) => node.into(),
        Some(NamedOrBlankNode::BlankNode(node)) => node.into(),
    })
}

pub(super) struct QuadInput {
    subject: TermTuple,
    predicate: TermTuple,
    object: TermTuple,
    graph: Option<TermTuple>,
}

impl QuadInput {
    pub fn new(
        subject: TermTuple,
        predicate: TermTuple,
        object: TermTuple,
        graph: Option<TermTuple>,
    ) -> Self {
        Self {
            subject,
            predicate,
            object,
            graph,
        }
    }

    pub fn quad(self) -> Result<GeneralizedQuad> {
        Ok(GeneralizedQuad::new(
            term(self.subject)?,
            term(self.predicate)?,
            term(self.object)?,
            graph(self.graph)?,
        ))
    }
}

#[cfg(test)]
#[path = "typed_tests.rs"]
mod tests;
