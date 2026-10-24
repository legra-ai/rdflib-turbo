//! Scoped RDFLib Graph NT formatting without narrowing generalized statement roles.
use std::fmt::Write;

use oxigraph::model::{BlankNodeRef, LiteralRef, NamedNodeRef, TermRef};
use oxigraph::store::{GeneralizedQuadIter, LexicalTerm};

use super::operations::runtime;
use crate::Result;

#[path = "nt_numeric.rs"]
mod numeric;

pub(super) struct NumericWarning {
    pub(super) value: String,
    pub(super) datatype: String,
}

pub(super) enum GraphNtOutput {
    Document(String),
    InvalidUri(String),
    WithWarnings(Vec<NumericWarning>, Box<GraphNtOutput>),
}

fn borrowed(term: &LexicalTerm) -> TermRef<'_> {
    match term {
        LexicalTerm::NamedNode(value) => NamedNodeRef::new_unchecked(value.as_str()).into(),
        LexicalTerm::BlankNode(value) => BlankNodeRef::new_unchecked(value.as_str()).into(),
        LexicalTerm::Literal {
            value,
            datatype,
            language,
        } => {
            let literal = if let Some(language) = language {
                LiteralRef::new_language_tagged_literal_unchecked(value.as_str(), language.as_str())
            } else if let Some(datatype) = datatype
                && !datatype.as_str().is_empty()
            {
                LiteralRef::new_typed_literal(
                    value.as_str(),
                    NamedNodeRef::new_unchecked(datatype.as_str()),
                )
            } else {
                LiteralRef::new_simple_literal(value.as_str())
            };
            literal.into()
        }
    }
}

fn invalid_uri(term: &LexicalTerm) -> Option<&str> {
    let LexicalTerm::NamedNode(value) = term else {
        return None;
    };
    value
        .as_str()
        .bytes()
        .any(|byte| {
            matches!(
                byte,
                b'<' | b'>' | b'"' | b' ' | b'{' | b'}' | b'|' | b'\\' | b'^' | b'`'
            )
        })
        .then(|| value.as_str())
}

fn n3_quote(value: &str) -> String {
    if value.contains('\n') {
        let mut encoded = value
            .replace('\\', "\\\\")
            .replace("\"\"\"", "\\\"\\\"\\\"");
        if encoded.ends_with('"') && !encoded.ends_with("\\\"") {
            encoded.insert(encoded.len() - 1, '\\');
        }
        format!("\"\"\"{}\"\"\"", encoded.replace('\r', "\\r"))
    } else {
        format!(
            "\"{}\"",
            value
                .replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('\r', "\\r")
        )
    }
}

fn n3_literal(term: &LexicalTerm, warnings: &mut Vec<NumericWarning>) -> Option<String> {
    let LexicalTerm::Literal {
        value,
        datatype,
        language,
    } = term
    else {
        return None;
    };
    let mut encoded = n3_quote(value.as_str());
    if let Some(datatype) = datatype
        && matches!(
            datatype.as_str(),
            "http://www.w3.org/2001/XMLSchema#decimal"
                | "http://www.w3.org/2001/XMLSchema#float"
                | "http://www.w3.org/2001/XMLSchema#double"
        )
    {
        match numeric::classify(value.as_str()) {
            numeric::Number::Infinite => {
                encoded = encoded.replace("inf", "INF").replace("Infinity", "INF")
            }
            numeric::Number::Nan => encoded = encoded.replace("nan", "NaN"),
            numeric::Number::Invalid => warnings.push(NumericWarning {
                value: value.as_str().into(),
                datatype: datatype.as_str().into(),
            }),
            numeric::Number::Finite => {}
        }
    }
    if let Some(language) = language
        && !language.as_str().is_empty()
    {
        write!(encoded, "@{}", language.as_str()).expect("String formatting is infallible");
        return Some(encoded);
    }
    if let Some(datatype) = datatype
        && !datatype.as_str().is_empty()
    {
        write!(encoded, "^^<{}>", datatype.as_str()).expect("String formatting is infallible");
    }
    Some(encoded)
}

pub(super) fn dump_graph_nt(quads: GeneralizedQuadIter<'_>) -> Result<GraphNtOutput> {
    let mut text = String::new();
    // Explicit diagnostic output: at most two N3 numeric warnings per serialized row.
    let mut warnings = Vec::new();
    for row in quads.project_lexical(true, true, true) {
        let row = row.map_err(runtime)?;
        for (index, term) in row.iter().enumerate() {
            let term = term.as_ref().expect("All triple roles were selected");
            if let Some(uri) = invalid_uri(term) {
                return Ok(with_warnings(
                    warnings,
                    GraphNtOutput::InvalidUri(uri.into()),
                ));
            }
            if index != 0 {
                text.push(' ');
            }
            if index < 2
                && let Some(literal) = n3_literal(term, &mut warnings)
            {
                text.push_str(&literal);
                continue;
            }
            write!(text, "{}", borrowed(term)).expect("String formatting is infallible");
        }
        text.push_str(" .\n");
    }
    Ok(with_warnings(warnings, GraphNtOutput::Document(text)))
}

fn with_warnings(warnings: Vec<NumericWarning>, output: GraphNtOutput) -> GraphNtOutput {
    if warnings.is_empty() {
        output
    } else {
        GraphNtOutput::WithWarnings(warnings, Box::new(output))
    }
}

#[cfg(test)]
mod tests {
    use super::{GraphNtOutput, dump_graph_nt};
    use oxigraph::model::{GeneralizedQuad, GraphName, Literal, NamedNode, Term};
    use oxigraph::store::Store;

    #[test]
    fn empty_datatype_nt_presentation_does_not_change_stored_identity() {
        for position in 0..3 {
            let store = Store::new().unwrap();
            let mut terms: [Term; 3] = [
                NamedNode::new_unchecked("urn:s").into(),
                NamedNode::new_unchecked("urn:p").into(),
                Literal::new_simple_literal("object").into(),
            ];
            terms[position] =
                Literal::new_typed_literal("value", NamedNode::new_unchecked("")).into();
            let quad = GeneralizedQuad::new(
                terms[0].clone(),
                terms[1].clone(),
                terms[2].clone(),
                GraphName::DefaultGraph,
            );
            store.insert_generalized(&quad).unwrap();
            let output = dump_graph_nt(store.generalized_quads_for_pattern(
                None,
                None,
                None,
                Some(GraphName::DefaultGraph.as_ref()),
            ))
            .unwrap();
            let GraphNtOutput::Document(text) = output else {
                panic!("Empty datatype is valid reference NT presentation");
            };
            let expected = match position {
                0 => "\"value\" <urn:p> \"object\" .\n",
                1 => "<urn:s> \"value\" \"object\" .\n",
                _ => "<urn:s> <urn:p> \"value\" .\n",
            };
            assert_eq!(text, expected);
            assert!(store.contains_generalized(&quad).unwrap());
        }
    }

    #[test]
    fn literal_n3_in_subject_and_predicate_retains_contextual_reference_formatting() {
        for (literal, n3) in [
            (
                Literal::new_simple_literal("line\nend\""),
                "\"\"\"line\nend\\\"\"\"\"",
            ),
            (
                Literal::new_typed_literal(
                    "nan",
                    NamedNode::new("http://www.w3.org/2001/XMLSchema#double").unwrap(),
                ),
                "\"NaN\"^^<http://www.w3.org/2001/XMLSchema#double>",
            ),
            (
                Literal::new_typed_literal("text", NamedNode::new_unchecked("urn:datatype space")),
                "\"text\"^^<urn:datatype space>",
            ),
        ] {
            for subject_position in [false, true] {
                let store = Store::new().unwrap();
                let named: Term = NamedNode::new("urn:node").unwrap().into();
                let (subject, predicate) = if subject_position {
                    (literal.clone().into(), named)
                } else {
                    (named, literal.clone().into())
                };
                store
                    .insert_generalized(&GeneralizedQuad::new(
                        subject,
                        predicate,
                        Literal::new_simple_literal("object"),
                        GraphName::DefaultGraph,
                    ))
                    .unwrap();
                let output = dump_graph_nt(store.generalized_quads_for_pattern(
                    None,
                    None,
                    None,
                    Some(GraphName::DefaultGraph.as_ref()),
                ))
                .unwrap();
                let GraphNtOutput::Document(text) = output else {
                    panic!("Valid contextual literal must serialize");
                };
                let expected = if subject_position {
                    format!("{n3} <urn:node> \"object\" .\n")
                } else {
                    format!("<urn:node> {n3} \"object\" .\n")
                };
                assert_eq!(text, expected);
            }
        }
    }
}
