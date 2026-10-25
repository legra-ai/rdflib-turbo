//! Programmatic shapes terms; syntax and ordinary data decoders remain strict.
use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::{Error, Result};

use crate::terms::Batch;

pub(crate) type Record = (u8, String, Option<String>, Option<String>);

pub(crate) fn term((kind, value, datatype, language): Record) -> Result<Term> {
    Ok(match (kind, datatype, language) {
        // RDFLib URIRef and BNode constructors preserve programmatic lexical
        // values, including relative/empty/nonstandard identifiers. This is
        // deliberately confined to shapes input, never a syntax-parser mode.
        (0, None, None) => NamedNode::new_unchecked(value).into(),
        (1, None, None) => BlankNode::new_unchecked(value).into(),
        (2, None, Some(language)) => Literal::new_language_tagged_literal(value, language)
            .map_err(|error| Error::Invalid(error.to_string()))?
            .into(),
        (2, datatype, None) => match datatype {
            Some(datatype) => {
                Literal::new_typed_literal(value, NamedNode::new_unchecked(datatype)).into()
            }
            None => Literal::new_simple_literal(value).into(),
        },
        _ => return Err(Error::Invalid("Invalid shapes term record".into())),
    })
}

pub(crate) struct DecodedShapes {
    terms: Vec<Term>,
    quads: Vec<(usize, usize, usize, Option<usize>)>,
}

impl DecodedShapes {
    pub fn new((records, quads): Batch) -> Result<Self> {
        Ok(Self {
            terms: records.into_iter().map(term).collect::<Result<_>>()?,
            quads,
        })
    }

    fn term(&self, index: usize) -> Result<TermRef<'_>> {
        self.terms
            .get(index)
            .map(Term::as_ref)
            .ok_or_else(|| Error::Invalid("Invalid shapes term index".into()))
    }

    pub fn quads(&self) -> impl Iterator<Item = Result<GeneralizedQuadRef<'_>>> {
        self.quads
            .iter()
            .map(|&(subject, predicate, object, graph)| {
                let graph = match graph.map(|index| self.term(index)).transpose()? {
                    None => GraphNameRef::DefaultGraph,
                    Some(TermRef::NamedNode(node)) => node.into(),
                    Some(TermRef::BlankNode(node)) => node.into(),
                    Some(TermRef::Literal(_)) => {
                        return Err(Error::Invalid(
                            "Shapes graph names must be IRIs or blank nodes".into(),
                        ));
                    }
                };
                Ok(GeneralizedQuadRef {
                    subject: self.term(subject)?,
                    predicate: self.term(predicate)?,
                    object: self.term(object)?,
                    graph_name: graph,
                })
            })
    }
}
