use std::collections::{HashMap, HashSet};

use oxigraph::model::*;

use crate::functions::Functions;
use crate::graph::*;
use crate::path::{Path, rdf_list};
use crate::{Error, GraphRead, Result};

#[cfg(test)]
#[path = "expression_tests.rs"]
mod tests;

pub(crate) enum NodeExpression {
    This,
    Constant(Term),
    Path(Path),
    Union(Vec<Self>),
    Intersection(Vec<Self>),
    Filter { shape: usize, nodes: Box<Self> },
    Function { name: NamedNode, args: Vec<Self> },
}

impl NodeExpression {
    pub fn read(
        graph: &(impl GraphRead + ?Sized),
        node: &Term,
        functions: &Functions,
    ) -> Result<crate::model::NodeExpression<Term>> {
        Self::read_inner(graph, node, functions, &mut HashSet::new())
    }

    fn read_inner(
        graph: &(impl GraphRead + ?Sized),
        node: &Term,
        functions: &Functions,
        stack: &mut HashSet<Term>,
    ) -> Result<crate::model::NodeExpression<Term>> {
        use crate::model::{ExpressionForm as K, NodeExpression as E};
        if node == &Term::from(iri(SH, "this")) {
            return Ok(E::focus_node().with_source(node.clone()));
        }
        if !matches!(node, Term::BlankNode(_)) {
            return Ok(E::constant(node.clone()).with_source(node.clone()));
        }
        if !stack.insert(node.clone()) {
            return Err(Error::Invalid("Cyclic SHACL node expression".into()));
        }
        let kind = if let Some(path) = one(graph, node, "path")? {
            K::PathValues {
                path: Path::read(graph, &path)?,
                path_node: Some(path),
                // pySHACL 0.40.1 evaluates sh:path from the focus, even when
                // sh:nodes is present. Preserve that established behavior.
                focus_node: None,
            }
        } else if let Some(list) = one(graph, node, "union")? {
            K::Concat(
                rdf_list(graph, &list)?
                    .iter()
                    .map(|n| Self::read_inner(graph, n, functions, stack).map(Box::new))
                    .collect::<Result<_>>()?,
            )
        } else if let Some(list) = one(graph, node, "intersection")? {
            K::Intersection(
                rdf_list(graph, &list)?
                    .iter()
                    .map(|n| Self::read_inner(graph, n, functions, stack).map(Box::new))
                    .collect::<Result<_>>()?,
            )
        } else if let Some(shape) = one(graph, node, "filterShape")? {
            let nodes = one(graph, node, "nodes")?
                .ok_or_else(|| Error::Invalid("sh:filterShape requires sh:nodes".into()))?;
            K::FilterShape {
                shape,
                nodes: Box::new(Self::read_inner(graph, &nodes, functions, stack)?),
            }
        } else {
            let mut calls = Vec::new();
            for quad in graph.matching_quads(
                Some(node.as_ref()),
                None,
                None,
                Some(GraphNameRef::DefaultGraph),
            ) {
                let quad = quad?;
                if let Term::NamedNode(predicate) = quad.predicate
                    && functions.contains(&predicate)
                {
                    calls.push((predicate, quad.object));
                }
            }
            if calls.len() != 1 {
                return Err(Error::Invalid(format!(
                    "Node expression {node} requires exactly one known function or expression operator"
                )));
            }
            let (name, list) = &calls[0];
            K::CustomList {
                function: crate::structural::named_role(name.clone().into(), "SHACL function")?,
                arguments: rdf_list(graph, list)?
                    .iter()
                    .map(|n| Self::read_inner(graph, n, functions, stack).map(Box::new))
                    .collect::<Result<_>>()?,
            }
        };
        stack.remove(node);
        E::try_new(kind)
            .map(|e| e.with_source(node.clone()))
            .map_err(|e| Error::Invalid(e.to_string()))
    }

    pub fn lower(
        expression: crate::model::NodeExpression<Term>,
        shapes: &HashMap<Term, usize>,
    ) -> Result<Self> {
        use crate::model::ExpressionForm as K;
        Ok(match expression.into_parts().1 {
            K::Var(name) if name.as_str() == "focusNode" => Self::This,
            K::Constant(value) => Self::Constant(value),
            K::PathValues {
                path,
                focus_node: None,
                ..
            } => Self::Path(Path::from(path)),
            K::PathValues {
                focus_node: Some(_),
                ..
            } => {
                return Err(Error::Unsupported(
                    "Nested path inputs are outside pySHACL's expression semantics".into(),
                ));
            }
            K::Concat(parts) => Self::Union(
                parts
                    .into_iter()
                    .map(|e| Self::lower(*e, shapes))
                    .collect::<Result<_>>()?,
            ),
            K::Intersection(parts) => Self::Intersection(
                parts
                    .into_iter()
                    .map(|e| Self::lower(*e, shapes))
                    .collect::<Result<_>>()?,
            ),
            K::FilterShape { shape, nodes } => Self::Filter {
                shape: *shapes
                    .get(&shape)
                    .ok_or_else(|| Error::Invalid("Unknown filter shape".into()))?,
                nodes: Box::new(Self::lower(*nodes, shapes)?),
            },
            K::CustomList {
                function: name,
                arguments: args,
            } => Self::Function {
                name,
                args: args
                    .into_iter()
                    .map(|e| Self::lower(*e, shapes))
                    .collect::<Result<_>>()?,
            },
            other => {
                return Err(Error::Unsupported(format!(
                    "Unsupported node expression: {other:?}"
                )));
            }
        })
    }

    pub fn evaluate(
        &self,
        focus: &Term,
        data: &(impl crate::GraphRead + ?Sized),
        functions: &Functions,
        conforms: &mut impl FnMut(usize, &Term) -> Result<bool>,
    ) -> Result<HashSet<Term>> {
        Ok(match self {
            Self::This => HashSet::from([focus.clone()]),
            Self::Constant(term) => HashSet::from([term.clone()]),
            Self::Path(path) => path.values(data, focus)?.into_iter().collect(),
            Self::Union(expressions) => {
                let mut nodes = HashSet::new();
                for expr in expressions {
                    nodes.extend(expr.evaluate(focus, data, functions, conforms)?);
                }
                nodes
            }
            Self::Intersection(expressions) => {
                let mut result = None::<HashSet<Term>>;
                for expr in expressions {
                    let nodes = expr.evaluate(focus, data, functions, conforms)?;
                    if let Some(result) = &mut result {
                        result.retain(|n| nodes.contains(n));
                    } else {
                        result = Some(nodes);
                    }
                }
                result.unwrap_or_default()
            }
            Self::Filter { shape, nodes } => {
                let mut result = HashSet::new();
                for node in nodes.evaluate(focus, data, functions, conforms)? {
                    if conforms(*shape, &node)? {
                        result.insert(node);
                    }
                }
                result
            }
            Self::Function { name, args } => {
                let mut products = vec![Vec::new()];
                for arg in args {
                    let values = arg.evaluate(focus, data, functions, conforms)?;
                    let mut next = Vec::new();
                    for product in products {
                        for value in &values {
                            let mut p = product.clone();
                            p.push(value.clone());
                            next.push(p);
                        }
                    }
                    products = next;
                }
                let mut result = HashSet::new();
                for product in products {
                    if let Some(value) = functions.call(name, &product, data)? {
                        result.insert(value);
                    }
                }
                result
            }
        })
    }
}
