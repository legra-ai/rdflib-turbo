use std::cmp::Ordering::{Greater, Less};
use std::fmt::Write as _;

use super::*;

impl CompiledShapes {
    /// Inspect targets using the same evaluator as validation and rules.
    /// The caller supplies the default-union view, reusable across calls.
    pub fn shape_focus_nodes(
        &self,
        node: &str,
        data: &(impl GraphRead + ?Sized),
        advanced: bool,
    ) -> Result<Vec<Term>> {
        let shape = self
            .shapes
            .iter()
            .find(|shape| shape.node.to_string() == node)
            .ok_or_else(|| Error::Invalid(format!("Unknown shape: {node}")))?;
        self.focus_nodes_with_advanced(shape, data, advanced, &mut Evaluation::new(15))
    }

    pub(super) fn focus_nodes(
        &self,
        shape: &Shape,
        data: &(impl GraphRead + ?Sized),
        evaluation: &mut Evaluation,
    ) -> Result<Vec<Term>> {
        self.focus_nodes_with_advanced(shape, data, true, evaluation)
    }

    /// Reverse declared targets for active node shapes. Data is a reusable
    /// default-union view. Skipped shapes are never evaluated.
    pub fn targeting_index(
        &self,
        data: &(impl GraphRead + ?Sized),
        skip: &HashSet<Term>,
    ) -> Result<HashMap<Term, Vec<Term>>> {
        let mut evaluation = Evaluation::new(15);
        let mut index: HashMap<Term, Vec<Term>> = HashMap::new();
        for shape in &self.shapes {
            if shape.deactivated || shape.path.is_some() || skip.contains(&shape.node) {
                continue;
            }
            for node in self.focus_nodes(shape, data, &mut evaluation)? {
                index.entry(node).or_default().push(shape.node.clone());
            }
        }
        for shapes in index.values_mut() {
            sort_focus_nodes(shapes);
        }
        Ok(index)
    }

    pub(super) fn focus_nodes_with_advanced(
        &self,
        shape: &Shape,
        data: &(impl GraphRead + ?Sized),
        advanced: bool,
        evaluation: &mut Evaluation,
    ) -> Result<Vec<Term>> {
        let mut nodes = HashSet::new();
        for target in &shape.targets {
            match target {
                Target::Node(node) => {
                    nodes.insert(node.clone());
                }
                Target::Class(class) => {
                    nodes.extend(evaluation.targets(data, class)?);
                }
                Target::Subjects(predicate) => {
                    nodes.extend(subjects(data, predicate, None)?);
                }
                Target::Objects(predicate) => {
                    for quad in data.matching_quads(
                        None,
                        Some(predicate.as_ref().into()),
                        None,
                        Some(GraphNameRef::DefaultGraph),
                    ) {
                        nodes.insert(quad?.object);
                    }
                }
                Target::Sparql { query, bindings } => {
                    if !advanced {
                        continue;
                    }
                    let functions = self.functions.clone();
                    let mut evaluation = query.prepare(&functions).on_queryable_dataset(data);
                    for (variable, value) in bindings {
                        evaluation =
                            evaluation.substitute_variable(variable.clone(), value.clone());
                    }
                    let QueryResults::Solutions(solutions) = evaluation.execute()? else {
                        return Err(Error::Invalid(
                            "SPARQL target did not return SELECT results".into(),
                        ));
                    };
                    if !solutions.variables().iter().any(|v| v.as_str() == "this") {
                        return Err(Error::Invalid("SPARQL target must project ?this".into()));
                    }
                    for solution in solutions {
                        let solution = solution?;
                        let node = solution.get("this").ok_or_else(|| {
                            Error::Invalid("SPARQL target returned an unbound ?this".into())
                        })?;
                        nodes.insert(node.clone());
                    }
                }
            }
        }
        let mut nodes: Vec<_> = nodes.into_iter().collect();
        sort_focus_nodes(&mut nodes);
        Ok(nodes)
    }

    pub(super) fn selected_focus_nodes(
        &self,
        shape: &Shape,
        data: &(impl GraphRead + ?Sized),
        options: &Options,
        evaluation: &mut Evaluation,
    ) -> Result<Vec<Term>> {
        if !options.use_shapes.is_empty() && !options.focus_nodes.is_empty() {
            return Ok(options.focus_nodes.clone());
        }
        let mut nodes = self.focus_nodes(shape, data, evaluation)?;
        if !options.focus_nodes.is_empty() {
            nodes.retain(|node| options.focus_nodes.contains(node));
        }
        Ok(nodes)
    }
}

/// Cache formatted literal keys once while keeping identifier-only sorting allocation-free.
fn sort_focus_nodes(nodes: &mut [Term]) {
    if nodes.iter().any(|node| matches!(node, Term::Literal(_))) {
        nodes.sort_by_cached_key(focus_display_key);
    } else {
        nodes.sort_by(focus_node_order);
    }
}

/// Reserve the unescaped payload once; RDF's formatter retains authority over escaping.
fn focus_display_key(node: &Term) -> String {
    let capacity = match node {
        Term::NamedNode(node) => node.as_str().len() + 2,
        Term::BlankNode(node) => node.as_str().len() + 2,
        Term::Literal(literal) => {
            literal.value().len()
                + literal.datatype().as_str().len()
                + literal.language().map_or(0, str::len)
                + 8
        }
    };
    let mut key = String::with_capacity(capacity);
    write!(&mut key, "{node}").expect("Formatting into a String is infallible");
    key
}

/// Compare the existing RDF display keys without repeatedly rendering identifiers.
fn focus_node_order(left: &Term, right: &Term) -> std::cmp::Ordering {
    match (left, right) {
        (Term::NamedNode(left), Term::NamedNode(right)) => {
            // Include the virtual closing delimiter: <urn:a0> sorts before <urn:a>.
            left.as_str()
                .bytes()
                .chain(std::iter::once(b'>'))
                .cmp(right.as_str().bytes().chain(std::iter::once(b'>')))
        }
        (Term::BlankNode(left), Term::BlankNode(right)) => left.as_str().cmp(right.as_str()),
        (Term::Literal(left), Term::Literal(right)) => left.to_string().cmp(&right.to_string()),
        // The display prefixes are respectively a quote, '<', and '_:'.
        (Term::Literal(_), _) | (Term::NamedNode(_), Term::BlankNode(_)) => Less,
        (_, Term::Literal(_)) | (Term::BlankNode(_), Term::NamedNode(_)) => Greater,
    }
}

#[cfg(test)]
#[path = "targets_order_tests.rs"]
mod order_tests;
