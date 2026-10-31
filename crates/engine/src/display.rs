//! Stable RDF descriptions for human-readable reports, independent of blank IDs.

use std::collections::{BTreeMap, HashMap, HashSet};

use bigdecimal::BigDecimal;
use num_bigint::BigInt;
use oxigraph::model::*;

use crate::graph::*;
use crate::{Error, GraphRead, Result};

pub(crate) struct Display<'a, G: GraphRead + ?Sized, S: GraphRead + ?Sized = Dataset> {
    pub shape_namespaces: Vec<(String, String)>,
    pub data_namespaces: Vec<(String, String)>,
    pub data: &'a G,
    pub shapes: &'a S,
}

impl<G: GraphRead + ?Sized, S: GraphRead + ?Sized> Display<'_, G, S> {
    pub fn iri(&self, node: &NamedNode) -> String {
        Self::iri_in(node, &self.shape_namespaces)
    }

    fn iri_in(node: &NamedNode, namespaces: &[(String, String)]) -> String {
        // RDFLib's URIRef.n3 rejects these characters before namespace lookup;
        // pySHACL renders such programmatic names as their raw lexical value.
        // This presentation rule neither validates nor changes the RDF term.
        if node
            .as_str()
            .chars()
            .any(|value| "<>\" {}|\\^`".contains(value))
        {
            return node.as_str().to_owned();
        }
        let mut candidates: Vec<_> = namespaces
            .iter()
            .filter_map(|(p, ns)| {
                node.as_str()
                    .strip_prefix(ns)
                    .filter(|local| {
                        !local.is_empty()
                            && local
                                .chars()
                                .all(|c| c.is_alphanumeric() || "_-.:".contains(c))
                    })
                    .map(|local| (ns.len(), format!("{p}:{local}")))
            })
            .collect();
        candidates.sort_by(|(a, x), (b, y)| b.cmp(a).then_with(|| x.cmp(y)));
        candidates
            .into_iter()
            .next()
            .map(|(_, s)| s)
            .unwrap_or_else(|| node.to_string())
    }

    pub fn term(&self, term: &Term, shapes: bool) -> Result<String> {
        if shapes {
            Self::describe(term, self.shapes, &self.shape_namespaces)
        } else {
            Self::describe(term, self.data, &self.data_namespaces)
        }
    }

    fn describe(
        term: &Term,
        graph: &(impl GraphRead + ?Sized),
        namespaces: &[(String, String)],
    ) -> Result<String> {
        let mut context = None;
        if let Term::BlankNode(node) = term {
            // bounded: retain only the best context for this reported blank node.
            for quad in graph.matching_quads(Some(node.as_ref().into()), None, None, None) {
                Self::select_context(&mut context, quad?.graph_name);
            }
            if context.is_none() {
                for quad in graph.matching_quads(None, None, Some(term.as_ref()), None) {
                    Self::select_context(&mut context, quad?.graph_name);
                }
            }
        }
        // bounded: memoize only descriptions emitted for this field; the cache
        // never outlives the render or its graph/context/namespaces.
        let mut completed = HashMap::new();
        Self::inner(
            term,
            graph,
            &context.unwrap_or(GraphName::DefaultGraph),
            namespaces,
            0,
            &mut completed,
        )
    }

    fn select_context(current: &mut Option<GraphName>, candidate: GraphName) {
        let key = |name: &GraphName| {
            (
                matches!(name, GraphName::NamedNode(n) if n.as_str() == "urn:pyshacl:inference"),
                !name.is_default_graph(),
                name.to_string(),
            )
        };
        if current
            .as_ref()
            .is_none_or(|selected| key(&candidate) < key(selected))
        {
            *current = Some(candidate);
        }
    }

    fn boolean_value(literal: &Literal) -> Option<&'static str> {
        if literal.datatype().as_str().strip_prefix(XSD) != Some("boolean") {
            return None;
        }
        match literal.value() {
            "true" | "1" => Some("True"),
            "false" | "0" => Some("False"),
            _ => None,
        }
    }

    fn integer_value(literal: &Literal) -> Option<String> {
        match literal.datatype().as_str().strip_prefix(XSD) {
            Some(
                "integer" | "long" | "int" | "short" | "byte" | "unsignedLong" | "unsignedInt"
                | "unsignedShort" | "unsignedByte" | "nonNegativeInteger" | "positiveInteger"
                | "nonPositiveInteger" | "negativeInteger",
            ) => literal
                .value()
                .parse::<BigInt>()
                .ok()
                .map(|value| value.to_string()),
            _ => None,
        }
    }

    pub fn message(&self, term: &Term) -> String {
        match term {
            Term::Literal(literal) => Self::boolean_value(literal)
                .map(str::to_owned)
                .or_else(|| Self::integer_value(literal))
                .unwrap_or_else(|| literal.value().to_owned()),
            _ => term.to_string(),
        }
    }

    fn literal(literal: &Literal, namespaces: &[(String, String)]) -> String {
        // Human-readable pySHACL descriptions contain raw lexical text, rather
        // than N-Triples escapes. RDF serialization keeps its separate formatter.
        let mut value = format!("\"{}\"", literal.value());
        let scalar = match literal.datatype().as_str().strip_prefix(XSD) {
            Some("boolean") => Self::boolean_value(literal).map(str::to_owned),
            Some("decimal") => literal
                .value()
                .parse::<BigDecimal>()
                .ok()
                .map(|value| value.to_string()),
            _ => Self::integer_value(literal),
        };
        if let Some(scalar) = scalar.filter(|scalar| scalar != literal.value()) {
            value.push_str(&format!(" = {scalar}"));
        }
        if let Some(language) = literal.language() {
            value.push_str(&format!(", lang={language}"));
        } else if literal.has_explicit_datatype() {
            value.push_str(&format!(
                ", datatype={}",
                Self::iri_in(&literal.datatype().into_owned(), namespaces)
            ));
        }
        format!("Literal({value})")
    }

    fn collection_value(
        graph: &(impl GraphRead + ?Sized),
        node: &Term,
        predicate: &NamedNode,
        context: &GraphName,
    ) -> Result<Term> {
        let mut values = graph.matching_quads(
            Some(subject(node)?.as_ref().into()),
            Some(predicate.as_ref().into()),
            None,
            Some(context.as_ref()),
        );
        let first = values.next().transpose()?;
        if first.is_none() || values.next().transpose()?.is_some() {
            return Err(Error::Invalid(format!("Malformed RDF list at {node}")));
        }
        Ok(first.expect("one list value checked above").object)
    }

    fn collection(
        head: &Term,
        graph: &(impl GraphRead + ?Sized),
        context: &GraphName,
        namespaces: &[(String, String)],
        depth: usize,
        completed: &mut HashMap<BlankNode, String>,
    ) -> Result<String> {
        // bounded: one visited cell per emitted collection item, never source-wide.
        let mut cells = HashSet::new();
        let mut node = head.clone();
        let mut text = String::from("( ");
        while node != Term::from(iri(RDF, "nil")) {
            if !cells.insert(node.clone()) {
                return Err(Error::Invalid(format!("Cyclic RDF list at {node}")));
            }
            let first = Self::collection_value(graph, &node, &iri(RDF, "first"), context)?;
            text.push_str(&Self::inner(
                &first,
                graph,
                context,
                namespaces,
                depth + 1,
                completed,
            )?);
            text.push(' ');
            node = Self::collection_value(graph, &node, &iri(RDF, "rest"), context)?;
        }
        text.push(')');
        Ok(text)
    }

    fn inner(
        term: &Term,
        graph: &(impl GraphRead + ?Sized),
        context: &GraphName,
        namespaces: &[(String, String)],
        depth: usize,
        completed: &mut HashMap<BlankNode, String>,
    ) -> Result<String> {
        Ok(match term {
            Term::NamedNode(n) => Self::iri_in(n, namespaces),
            Term::Literal(literal) => Self::literal(literal, namespaces),
            Term::BlankNode(node) => {
                // pySHACL guards new expansions before consulting its cache.
                // Completed subtrees retain their text when reused below the
                // limit, even if a fresh expansion here would truncate them.
                if depth >= 6 {
                    return Ok("<http://recursion.too.deep>".into());
                }
                if let Some(description) = completed.get(node) {
                    return Ok(description.clone());
                }
                let first = graph
                    .matching_quads(
                        Some(subject(term)?.as_ref().into()),
                        Some(iri(RDF, "first").as_ref().into()),
                        None,
                        Some(context.as_ref()),
                    )
                    .next()
                    .transpose()?;
                if first.is_some() {
                    return Self::collection(term, graph, context, namespaces, depth, completed);
                }
                let mut properties = BTreeMap::<String, Vec<Term>>::new();
                for q in graph.matching_quads(
                    Some(subject(term).expect("blank node").as_ref().into()),
                    None,
                    None,
                    Some(context.as_ref()),
                ) {
                    let q = q?;
                    properties
                        .entry(Self::inner(
                            &q.predicate,
                            graph,
                            context,
                            namespaces,
                            depth + 1,
                            completed,
                        )?)
                        .or_default()
                        .push(q.object);
                }
                let mut descriptions = Vec::new();
                // Populate the memo in displayed predicate order rather than
                // allowing physical index traversal to choose the first path.
                for (p, objects) in properties {
                    let mut values = Vec::new();
                    for object in objects {
                        values.push(Self::inner(
                            &object,
                            graph,
                            context,
                            namespaces,
                            depth + 1,
                            completed,
                        )?);
                    }
                    values.sort();
                    descriptions.push(format!("{p} {}", values.join(", ")));
                }
                if descriptions.is_empty() {
                    return Ok("[ ]".into());
                }
                let description = format!("[ {} ]", descriptions.join(" ; "));
                completed.insert(node.clone(), description.clone());
                description
            }
        })
    }
}

#[cfg(test)]
#[path = "display_tests.rs"]
mod tests;
