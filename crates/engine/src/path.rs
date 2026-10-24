use std::collections::HashSet;

use oxigraph::model::*;

use crate::graph::*;
use crate::model::ShaclPath;
use crate::{Error, GraphRead, Result};

#[derive(Clone, Eq, PartialEq, Hash)]
pub(crate) enum Path {
    Predicate(NamedNode),
    Sequence(Vec<Path>),
    Alternative(Vec<Path>),
    Inverse(Box<Path>),
    ZeroOrMore(Box<Path>),
    OneOrMore(Box<Path>),
    ZeroOrOne(Box<Path>),
}

pub(crate) fn rdf_list(store: &(impl GraphRead + ?Sized), head: &Term) -> Result<Vec<Term>> {
    let mut node = head.clone();
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    while node != Term::from(iri(RDF, "nil")) {
        subject(&node)?;
        if !seen.insert(node.clone()) {
            return Err(Error::Invalid(format!("Cyclic RDF list at {node}")));
        }
        let first = objects(store, &node, &iri(RDF, "first"))?;
        let rest = objects(store, &node, &iri(RDF, "rest"))?;
        if first.len() != 1 || rest.len() != 1 {
            return Err(Error::Invalid(format!("Malformed RDF list at {node}")));
        }
        result.push(first[0].clone());
        node = rest[0].clone();
    }
    Ok(result)
}

impl Path {
    pub fn read(store: &(impl GraphRead + ?Sized), node: &Term) -> Result<ShaclPath> {
        Self::parse(store, node, &mut HashSet::new())
    }

    fn parse(
        store: &(impl GraphRead + ?Sized),
        node: &Term,
        active: &mut HashSet<Term>,
    ) -> Result<ShaclPath> {
        if let Term::NamedNode(p) = node {
            return Ok(ShaclPath::Predicate(crate::structural::named_role(
                p.clone().into(),
                "Path predicate",
            )?));
        }
        if matches!(node, Term::Literal(_)) {
            return Err(Error::ShapeLoad {
                message: "Values of a property path cannot be a Literal.",
                link: "https://www.w3.org/TR/shacl/#property-paths",
            });
        }
        if !matches!(node, Term::BlankNode(_)) {
            return Err(Error::Invalid(format!("Invalid SHACL path {node}")));
        }
        if active.len() >= 256 {
            return Err(Error::Invalid("Path nesting limit exceeded".into()));
        }
        if !active.insert(node.clone()) {
            return Err(Error::Invalid(format!("Cyclic path definition at {node}")));
        }
        let sequence = !objects(store, node, &iri(RDF, "first"))?.is_empty();
        // The sequence rule permits additional triples. Other path forms must
        // be the subject of exactly one triple, so cannot compete with a list.
        if sequence {
            let path = ShaclPath::Sequence(Self::path_list(store, node, active)?);
            active.remove(node);
            return Ok(path);
        }
        let mut operators = Vec::new();
        for name in [
            "inversePath",
            "alternativePath",
            "zeroOrMorePath",
            "oneOrMorePath",
            "zeroOrOnePath",
        ] {
            if let Some(value) = one(store, node, name)? {
                operators.push((name, value));
            }
        }
        let subject_rows = store
            .matching_quads(
                Some(subject(node)?.as_ref().into()),
                None,
                None,
                Some(GraphNameRef::DefaultGraph),
            )
            .try_fold(0usize, |count, row| row.map(|_| count + 1))?;
        if operators.len() != 1 || subject_rows != 1 {
            return Err(Error::Invalid(format!(
                "Path {node} must have exactly one path form"
            )));
        }
        let result = if sequence {
            ShaclPath::Sequence(Self::path_list(store, node, active)?)
        } else {
            let (name, value) = &operators[0];
            if *name == "alternativePath" {
                ShaclPath::Alternative(Self::path_list(store, value, active)?)
            } else {
                let path = Box::new(Self::parse(store, value, active)?);
                match *name {
                    "inversePath" => ShaclPath::Inverse(path),
                    "zeroOrMorePath" => ShaclPath::ZeroOrMore(path),
                    "oneOrMorePath" => ShaclPath::OneOrMore(path),
                    "zeroOrOnePath" => ShaclPath::ZeroOrOne(path),
                    _ => unreachable!(),
                }
            }
        };
        active.remove(node);
        Ok(result)
    }

    fn path_list(
        store: &(impl GraphRead + ?Sized),
        node: &Term,
        active: &mut HashSet<Term>,
    ) -> Result<Vec<ShaclPath>> {
        let terms = rdf_list(store, node)?;
        if terms.len() < 2 {
            return Err(Error::Invalid(
                "Sequence/alternative path needs at least two members".into(),
            ));
        }
        terms
            .iter()
            .map(|n| Self::parse(store, n, active))
            .collect()
    }

    pub fn values(&self, data: &(impl GraphRead + ?Sized), focus: &Term) -> Result<Vec<Term>> {
        #[cfg(test)]
        PATH_EVALUATIONS.with(|count| count.set(count.get() + 1));
        let mut result: Vec<_> = self.walk(data, focus, false)?.into_iter().collect();
        result.sort_by_key(ToString::to_string);
        Ok(result)
    }

    fn walk(
        &self,
        data: &(impl GraphRead + ?Sized),
        focus: &Term,
        reverse: bool,
    ) -> Result<HashSet<Term>> {
        match self {
            Self::Predicate(p) => Ok(if reverse {
                subjects(data, p, Some(focus))?
            } else {
                objects(data, focus, p)?
            }
            .into_iter()
            .collect()),
            Self::Inverse(path) => path.walk(data, focus, !reverse),
            Self::Alternative(paths) => {
                let mut found = HashSet::new();
                for path in paths {
                    found.extend(path.walk(data, focus, reverse)?);
                }
                Ok(found)
            }
            Self::Sequence(paths) => {
                let mut found = HashSet::from([focus.clone()]);
                for index in 0..paths.len() {
                    let path = &paths[if reverse {
                        paths.len() - 1 - index
                    } else {
                        index
                    }];
                    let mut next = HashSet::new();
                    for value in found {
                        next.extend(path.walk(data, &value, reverse)?);
                    }
                    found = next;
                }
                Ok(found)
            }
            Self::ZeroOrOne(path) => {
                let mut found = path.walk(data, focus, reverse)?;
                found.insert(focus.clone());
                Ok(found)
            }
            Self::ZeroOrMore(path) | Self::OneOrMore(path) => {
                let mut found = HashSet::new();
                let mut pending = vec![focus.clone()];
                let mut visited = HashSet::new();
                if matches!(self, Self::ZeroOrMore(_)) {
                    found.insert(focus.clone());
                }
                while let Some(value) = pending.pop() {
                    if !visited.insert(value.clone()) {
                        continue;
                    }
                    for next in path.walk(data, &value, reverse)? {
                        found.insert(next.clone());
                        pending.push(next);
                    }
                }
                Ok(found)
            }
        }
    }
}

#[cfg(test)]
thread_local! {
    pub(crate) static PATH_EVALUATIONS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl From<ShaclPath> for Path {
    fn from(path: ShaclPath) -> Self {
        match path {
            ShaclPath::Predicate(p) => Self::Predicate(p),
            ShaclPath::Sequence(paths) => {
                Self::Sequence(paths.into_iter().map(Self::from).collect())
            }
            ShaclPath::Alternative(paths) => {
                Self::Alternative(paths.into_iter().map(Self::from).collect())
            }
            ShaclPath::Inverse(p) => Self::Inverse(Box::new(Self::from(*p))),
            ShaclPath::ZeroOrMore(p) => Self::ZeroOrMore(Box::new(Self::from(*p))),
            ShaclPath::OneOrMore(p) => Self::OneOrMore(Box::new(Self::from(*p))),
            ShaclPath::ZeroOrOne(p) => Self::ZeroOrOne(Box::new(Self::from(*p))),
        }
    }
}
