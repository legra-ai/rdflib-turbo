use std::collections::HashSet;

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::*;

use crate::{Error, GraphRead, Result};

#[cfg(test)]
#[path = "graph_tests.rs"]
mod tests;

pub const SH: &str = "http://www.w3.org/ns/shacl#";
pub const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
pub const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
pub const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

pub fn iri(namespace: &str, name: &str) -> NamedNode {
    NamedNode::new_unchecked(format!("{namespace}{name}"))
}

pub fn subject(term: &Term) -> Result<NamedOrBlankNode> {
    match term {
        Term::NamedNode(n) => Ok(n.clone().into()),
        Term::BlankNode(n) => Ok(n.clone().into()),
        _ => Err(Error::Invalid(format!(
            "Expected a shape IRI or blank node, got {term}"
        ))),
    }
}

/// Preserve blank-node identities and literal spellings/types while parsing.
/// Oxigraph Store would canonicalize terms; oxrdf Dataset retains exact terms.
pub fn parse(input: &str, format: RdfFormat, base_iri: Option<&str>) -> Result<Dataset> {
    parse_with_graphs(input, format, base_iri, false, false)
}

pub fn parse_dataset(input: &str, format: RdfFormat, base_iri: Option<&str>) -> Result<Dataset> {
    parse_with_graphs(input, format, base_iri, true, false)
}

/// Store APIs expose RDF 1.1 strings as explicit xsd:string to RDFLib callers.
/// Their N-Quads writer abbreviates the datatype, so restore that adapter
/// identity while parsing, without altering ordinary document/Graph input.
pub fn parse_store_snapshot(input: &str) -> Result<Dataset> {
    parse_with_graphs(input, RdfFormat::NQuads, None, true, true)
}

pub fn store_quad(mut quad: Quad) -> Quad {
    if let Term::Literal(literal) = &quad.object
        && literal.datatype() == iri(XSD, "string").as_ref()
        && !literal.has_explicit_datatype()
    {
        quad.object = Literal::new_typed_literal(literal.value(), iri(XSD, "string")).into();
    }
    quad
}

fn parse_with_graphs(
    input: &str,
    format: RdfFormat,
    base_iri: Option<&str>,
    named: bool,
    store_strings: bool,
) -> Result<Dataset> {
    let mut store = Dataset::new();
    let mut parser = RdfParser::from_format(format);
    if !named {
        parser = parser.without_named_graphs();
    }
    if let Some(base) = base_iri {
        parser = parser
            .with_base_iri(base)
            .map_err(|e| Error::Invalid(format!("Invalid base IRI: {e}")))?;
    }
    let quads = parser
        .for_slice(input.as_bytes())
        .map(|q| q.map(|q| if store_strings { store_quad(q) } else { q }))
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::Invalid(format!("RDF parse error: {e}")))?;
    store.extend(quads);
    Ok(store)
}

pub fn serialize(store: &Dataset) -> Result<String> {
    let mut writer =
        oxigraph::io::RdfSerializer::from_format(RdfFormat::NQuads).for_writer(Vec::new());
    for quad in store.iter() {
        writer
            .serialize_quad(quad)
            .map_err(|e| Error::Runtime(e.to_string()))?;
    }
    String::from_utf8(writer.finish().map_err(|e| Error::Runtime(e.to_string()))?)
        .map_err(|e| Error::Runtime(e.to_string()))
}

#[cfg(test)]
thread_local! {
    pub(crate) static UNION_BUILDS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Default-union view retaining the original named graphs for GRAPH clauses.
pub fn with_union(data: &Dataset) -> Dataset {
    #[cfg(test)]
    UNION_BUILDS.with(|count| count.set(count.get() + 1));
    let mut union = data.clone();
    union.extend(
        data.iter()
            .map(|q| Quad::new(q.subject, q.predicate, q.object, GraphName::DefaultGraph)),
    );
    union
}

pub fn commit_additions(data: &mut Dataset, before: &Dataset, after: &Dataset) {
    data.extend(
        after
            .iter()
            .filter(|q| !before.contains(*q))
            .map(|q| q.into_owned()),
    );
}

pub fn objects(
    store: &(impl GraphRead + ?Sized),
    node: &Term,
    predicate: &NamedNode,
) -> Result<Vec<Term>> {
    store
        .matching_quads(
            Some(node.as_ref()),
            Some(predicate.as_ref().into()),
            None,
            Some(GraphNameRef::DefaultGraph),
        )
        .map(|q| q.map(|q| q.object))
        .collect()
}

pub fn values(store: &(impl GraphRead + ?Sized), node: &Term, name: &str) -> Result<Vec<Term>> {
    objects(store, node, &iri(SH, name))
}

pub fn one(store: &(impl GraphRead + ?Sized), node: &Term, name: &str) -> Result<Option<Term>> {
    let mut terms = values(store, node, name)?;
    if terms.len() > 1 {
        return Err(Error::Invalid(format!(
            "{node} has multiple sh:{name} values"
        )));
    }
    Ok(terms.pop())
}

pub fn boolean(store: &(impl GraphRead + ?Sized), node: &Term, name: &str) -> Result<bool> {
    match one(store, node, name)? {
        None => Ok(false),
        Some(Term::Literal(v)) if v.datatype() == iri(XSD, "boolean").as_ref() => match v.value() {
            "true" | "1" => Ok(true),
            "false" | "0" => Ok(false),
            _ => Err(Error::Invalid(format!(
                "Invalid sh:{name} boolean on {node}"
            ))),
        },
        _ => Err(Error::Invalid(format!(
            "sh:{name} must be an xsd:boolean on {node}"
        ))),
    }
}

pub fn subjects(
    store: &(impl GraphRead + ?Sized),
    predicate: &NamedNode,
    object: Option<&Term>,
) -> Result<Vec<Term>> {
    store
        .matching_quads(
            None,
            Some(predicate.as_ref().into()),
            object.map(Term::as_ref),
            Some(GraphNameRef::DefaultGraph),
        )
        .map(|q| q.map(|q| q.subject))
        .collect()
}

pub fn add(store: &mut Dataset, s: &Term, p: NamedNode, o: Term) -> Result<()> {
    store.insert(&Quad::new(subject(s)?, p, o, GraphName::DefaultGraph));
    Ok(())
}

/// Cache directly referenced roots by source, preserving their identity. Each
/// descendant occurrence is fresh; reference report cloning stops at depth 10.
#[derive(Default)]
pub(crate) struct ReportClones(HashSet<(usize, Term)>);

impl ReportClones {
    pub fn copy(
        &mut self,
        source: &(impl GraphRead + ?Sized),
        target: &mut Dataset,
        node: &Term,
    ) -> Result<()> {
        if !matches!(node, Term::BlankNode(_))
            || !self.0.insert((
                std::ptr::from_ref(source).cast::<()>() as usize,
                node.clone(),
            ))
        {
            return Ok(());
        }
        Self::blank(source, target, node, 0, true)?;
        Ok(())
    }

    fn blank(
        source: &(impl GraphRead + ?Sized),
        target: &mut Dataset,
        node: &Term,
        depth: usize,
        keep_id: bool,
    ) -> Result<Term> {
        let cloned = if keep_id {
            node.clone()
        } else {
            BlankNode::default().into()
        };
        // bounded: reference child calls add two; at most six blank frames.
        if depth >= 10 {
            return Ok(cloned);
        }
        if Self::value(source, node, &iri(RDF, "first"))?.is_some() {
            Self::list(source, target, node, &cloned, depth)?;
            return Ok(cloned);
        }
        // bounded: deduplicate outgoing report-description edges across contexts.
        let mut seen = HashSet::new();
        for quad in source.matching_quads(Some(node.as_ref()), None, None, None) {
            let quad = quad?;
            if !seen.insert((quad.predicate.clone(), quad.object.clone())) {
                continue;
            }
            let object = Self::object(source, target, &quad.object, depth + 2)?;
            let predicate = crate::constraints::named(quad.predicate, "report predicate")?;
            add(target, &cloned, predicate, object)?;
        }
        Ok(cloned)
    }

    fn object(
        source: &(impl GraphRead + ?Sized),
        target: &mut Dataset,
        node: &Term,
        depth: usize,
    ) -> Result<Term> {
        if matches!(node, Term::BlankNode(_)) {
            Self::blank(source, target, node, depth, false)
        } else {
            Ok(node.clone())
        }
    }

    fn value(
        source: &(impl GraphRead + ?Sized),
        node: &Term,
        predicate: &NamedNode,
    ) -> Result<Option<Term>> {
        Ok(source
            .matching_quads(
                Some(node.as_ref()),
                Some(predicate.as_ref().into()),
                None,
                None,
            )
            .next()
            .transpose()?
            .map(|quad| quad.object))
    }

    fn list(
        source: &(impl GraphRead + ?Sized),
        target: &mut Dataset,
        head: &Term,
        cloned: &Term,
        depth: usize,
    ) -> Result<()> {
        let mut node = Some(head.clone());
        let mut previous = None;
        // bounded: one visited cell per emitted list item, no source-wide clone.
        let mut seen = HashSet::new();
        while let Some(current) = node {
            if !seen.insert(current.clone()) {
                return Err(Error::Invalid(format!("Cyclic RDF list at {current}")));
            }
            if let Some(item) = Self::value(source, &current, &iri(RDF, "first"))? {
                let cell = if previous.is_none() {
                    cloned.clone()
                } else {
                    BlankNode::default().into()
                };
                if let Some(previous) = previous {
                    add(target, &previous, iri(RDF, "rest"), cell.clone())?;
                }
                let item = Self::object(source, target, &item, depth + 2)?;
                add(target, &cell, iri(RDF, "first"), item)?;
                previous = Some(cell);
            }
            node = Self::value(source, &current, &iri(RDF, "rest"))?;
        }
        if let Some(previous) = previous {
            add(target, &previous, iri(RDF, "rest"), iri(RDF, "nil").into())?;
        }
        Ok(())
    }
}

pub(crate) fn instances(graph: &(impl GraphRead + ?Sized), class: &Term) -> Result<HashSet<Term>> {
    let mut result = HashSet::new();
    let mut seen = HashSet::new();
    let mut pending = vec![class.clone()];
    while let Some(class) = pending.pop() {
        if !seen.insert(class.clone()) {
            continue;
        }
        result.extend(subjects(graph, &iri(RDF, "type"), Some(&class))?);
        pending.extend(subjects(graph, &iri(RDFS, "subClassOf"), Some(&class))?);
    }
    Ok(result)
}
