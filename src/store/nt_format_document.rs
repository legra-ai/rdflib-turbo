//! N-Triples documents of one graph with rdflib's numeric spellings and warnings.
use std::fmt::Write;

use oxigraph::model::{BlankNodeRef, LiteralRef, NamedNodeRef, TermRef};
use oxigraph::store::{GeneralizedQuadIter, LexicalTerm};

use super::super::operations::runtime;
use super::numeric;
use crate::Result;

pub(in crate::store) struct NumericWarning {
    pub(in crate::store) value: String,
    pub(in crate::store) datatype: String,
}

pub(in crate::store) enum GraphNtOutput {
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

pub(in crate::store) fn dump_graph_nt(quads: GeneralizedQuadIter<'_>) -> Result<GraphNtOutput> {
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
