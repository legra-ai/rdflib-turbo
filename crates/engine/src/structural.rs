//! Read individual SHACL declarations into the shared structural model.
//! The original RDF dataset remains the report/source graph, not an alternate
//! compiler path. Execution structures consume these declarations by value.

use std::collections::HashMap;

use crate::model;
use oxigraph::model::*;

use crate::components::Component as CustomComponent;
use crate::constraints::named;
use crate::expression::NodeExpression;
use crate::functions::Functions;
use crate::graph::*;
use crate::path::rdf_list;
use crate::{Error, GraphRead, Result};

#[cfg(test)]
#[path = "structural_boundary_tests.rs"]
mod boundary_tests;

pub(crate) enum ShapeDeclaration {
    Node(Box<model::NodeShape<Term>>),
    Property(Box<model::PropertyShape<Term>>),
}

pub(crate) fn named_role(value: Term, context: &str) -> Result<NamedNode> {
    named(value, context)
}

pub(crate) fn query(
    graph: &(impl GraphRead + ?Sized),
    node: &Term,
    text: Term,
) -> Result<model::SparqlQuery<Term>> {
    Ok(model::SparqlQuery {
        text,
        prefixes: values(graph, node, "prefixes")?,
    })
}

pub(crate) fn messages(graph: &(impl GraphRead + ?Sized), node: &Term) -> Result<Vec<Term>> {
    let messages = values(graph, node, "message")?;
    if messages.iter().any(|m| !matches!(m, Term::Literal(_))) {
        return Err(Error::Invalid(format!(
            "sh:message must be a literal on {node}"
        )));
    }
    Ok(messages)
}

pub(crate) fn read_shape(
    graph: &(impl GraphRead + ?Sized),
    node: &Term,
    paths: &HashMap<Term, model::ShaclPath>,
    advanced: bool,
    functions: &Functions,
    custom_components: &[CustomComponent],
    digits: crate::constraints::DecimalDigitPolicy,
) -> Result<ShapeDeclaration> {
    let types = objects(graph, node, &iri(RDF, "type"))?;
    let paths_for_node = values(graph, node, "path")?;
    if types.contains(&iri(SH, "NodeShape").into()) && !paths_for_node.is_empty() {
        return Err(Error::ShapeLoad {
            message: "A shape defined as a NodeShape cannot be the subject of a 'sh:path' predicate.",
            link: "https://www.w3.org/TR/shacl/#node-shapes",
        });
    }
    let property_shape = types.contains(&iri(SH, "PropertyShape").into());
    if property_shape && paths_for_node.is_empty() {
        return Err(Error::ShapeLoad {
            message: "A shape defined as a PropertyShape must include one `sh:path` property.",
            link: "https://www.w3.org/TR/shacl/#property-shapes",
        });
    }
    let path_node = match paths_for_node.as_slice() {
        [] => None,
        [value] => Some(value.clone()),
        _ if property_shape => {
            return Err(Error::ShapeLoad {
                message: "A shape defined as a PropertyShape cannot have more than one 'sh:path' property.",
                link: "https://www.w3.org/TR/shacl/#property-shapes",
            });
        }
        _ => {
            return Err(Error::Invalid(format!(
                "{node} has multiple sh:path values"
            )));
        }
    };
    let mut shape = model::NodeShape::new(node.clone());
    shape.targets = targets(graph, node, advanced)?;
    if types.contains(&iri(RDFS, "Class").into())
        || types.contains(&iri("http://www.w3.org/2002/07/owl#", "Class").into())
    {
        shape
            .targets
            .push(model::Target::ImplicitClass(node.clone()));
    }
    shape.components = crate::constraints::read(graph, node, path_node.is_some(), digits)?;
    if advanced {
        for expression in values(graph, node, "expression")? {
            shape.components.push(model::Component::Expression(Box::new(
                model::ExpressionConstraint {
                    expression: NodeExpression::read(graph, &expression, functions)?,
                    messages: values(graph, &expression, "message")?,
                },
            )));
        }
    }
    for constraint in values(graph, node, "sparql")? {
        shape
            .components
            .push(model::Component::Sparql(Box::new(sparql_constraint(
                graph,
                &constraint,
            )?)));
    }
    for component in custom_components {
        if let Some(application) = component.read_application(graph, node)? {
            shape.components.push(model::Component::Custom(application));
        }
    }
    shape.property_shapes = values(graph, node, "property")?;
    // Core validation ignores SHACL-AF rules, including malformed bodies.
    // Do not parse declarations which cannot execute in this program.
    if advanced {
        shape.rules = values(graph, node, "rule")?
            .into_iter()
            .map(|node| rule(graph, &node, functions))
            .collect::<Result<_>>()?;
    }
    shape.messages = messages(graph, node)?;
    shape.severity = match one(graph, node, "severity")? {
        Some(value) => model::Severity::from_iri(named_role(value, "sh:severity")?.as_str())
            .map_err(|e| Error::Invalid(e.to_string()))?,
        None => model::Severity::Violation,
    };
    shape.order = one(graph, node, "order")?;
    shape.deactivated = boolean(graph, node, "deactivated")?;
    Ok(match path_node {
        None => ShapeDeclaration::Node(Box::new(shape)),
        Some(path_node) => ShapeDeclaration::Property(Box::new(model::PropertyShape {
            id: shape.id,
            path: paths
                .get(&path_node)
                .ok_or_else(|| Error::Invalid(format!("Unresolved path {path_node}")))?
                .clone(),
            path_node: Some(path_node),
            targets: shape.targets,
            components: shape.components,
            property_shapes: shape.property_shapes,
            deactivated: shape.deactivated,
            severity: shape.severity,
            names: values(graph, node, "name")?,
            descriptions: values(graph, node, "description")?,
            order: shape.order,
            // Display defaults are not execution inputs. Keep their source
            // triples without imposing new cardinality checks during validation.
            default_value: None,
            messages: shape.messages,
            rules: shape.rules,
        })),
    })
}

fn targets(
    graph: &(impl GraphRead + ?Sized),
    node: &Term,
    advanced: bool,
) -> Result<Vec<model::Target<Term>>> {
    use model::Target;
    let mut targets: Vec<_> = values(graph, node, "targetNode")?
        .into_iter()
        .map(Target::Node)
        .collect();
    for name in ["targetClass", "targetSubjectsOf", "targetObjectsOf"] {
        for value in values(graph, node, name)? {
            let value = named_role(value, &format!("sh:{name}"))?;
            targets.push(match name {
                "targetClass" => Target::Class(value),
                "targetSubjectsOf" => Target::SubjectsOf(value),
                "targetObjectsOf" => Target::ObjectsOf(value),
                _ => unreachable!(),
            });
        }
    }
    if !advanced {
        return Ok(targets);
    }
    for target in values(graph, node, "target")? {
        let types = objects(graph, &target, &iri(RDF, "type"))?;
        if types.contains(&iri(SH, "SPARQLTarget").into()) {
            targets.push(Target::Sparql(Box::new(model::SparqlTarget::new(
                target.clone(),
                select(graph, &target)?,
            ))));
        } else {
            let definition = types
                .into_iter()
                .find(|t| {
                    objects(graph, t, &iri(RDF, "type"))
                        .is_ok_and(|v| v.contains(&iri(SH, "SPARQLTargetType").into()))
                })
                .ok_or_else(|| Error::Unsupported(format!("target type on {target}")))?;
            let parameters = crate::functions::read_parameters(graph, &definition)?;
            let supplied = parameters
                .into_iter()
                .map(|p| {
                    Ok(model::ParameterValues {
                        values: objects(graph, &target, &p.path.clone())?,
                        path: p.path,
                    })
                })
                .collect::<Result<_>>()?;
            targets.push(Target::Custom(model::CustomTarget {
                id: target,
                target_type: definition,
                parameters: supplied,
            }));
        }
    }
    Ok(targets)
}

pub(crate) fn select(
    graph: &(impl GraphRead + ?Sized),
    node: &Term,
) -> Result<model::SparqlQuery<Term>> {
    let text = one(graph, node, "select")?
        .ok_or_else(|| Error::Invalid(format!("Missing sh:select on {node}")))?;
    query(graph, node, text)
}

pub(crate) fn sparql_constraint(
    graph: &(impl GraphRead + ?Sized),
    node: &Term,
) -> Result<model::SparqlConstraint<Term>> {
    let mut declaration = model::SparqlConstraint::new(node.clone(), select(graph, node)?);
    declaration.messages = messages(graph, node)?;
    declaration.deactivated = boolean(graph, node, "deactivated")?;
    Ok(declaration)
}

pub(crate) fn validator(
    graph: &(impl GraphRead + ?Sized),
    node: &Term,
    ask: bool,
) -> Result<model::SparqlValidator<Term>> {
    let query_name = if ask { "ask" } else { "select" };
    let text = one(graph, node, query_name)?
        .ok_or_else(|| Error::Invalid(format!("Missing sh:{query_name} on {node}")))?;
    let query = query(graph, node, text)?;
    let query = if ask {
        model::SparqlSelectOrAsk::Ask(query)
    } else {
        model::SparqlSelectOrAsk::Select(query)
    };
    let mut declaration = model::SparqlValidator::new(node.clone(), query);
    declaration.messages = messages(graph, node)?;
    Ok(declaration)
}

pub(crate) fn target_type(
    graph: &(impl GraphRead + ?Sized),
    node: &Term,
) -> Result<model::SparqlTargetType<Term>> {
    let mut target = model::SparqlTargetType::new(node.clone(), select(graph, node)?);
    target.parameters = crate::functions::read_parameters(graph, node)?;
    Ok(target)
}

fn rule(
    graph: &(impl GraphRead + ?Sized),
    node: &Term,
    functions: &Functions,
) -> Result<model::ShaclRule<Term>> {
    let types = objects(graph, node, &iri(RDF, "type"))?;
    // pySHACL selects the SPARQL role when a resource declares both kinds.
    let kinds = if types.contains(&iri(SH, "SPARQLRule").into()) {
        let queries = values(graph, node, "construct")?;
        if queries.is_empty() {
            return Err(Error::Invalid(format!("Missing sh:construct on {node}")));
        }
        queries
            .into_iter()
            .map(|text| query(graph, node, text).map(model::RuleKind::Sparql))
            .collect::<Result<_>>()?
    } else if types.contains(&iri(SH, "TripleRule").into()) {
        let expression = |name| {
            let n = one(graph, node, name)?
                .ok_or_else(|| Error::Invalid(format!("TripleRule requires sh:{name}")))?;
            NodeExpression::read(graph, &n, functions)
        };
        vec![model::RuleKind::Triple(Box::new(model::TripleRule {
            subject: expression("subject")?,
            predicate: expression("predicate")?,
            object: expression("object")?,
        }))]
    } else {
        return Err(Error::Unsupported(format!("rule type on {node}")));
    };
    let mut conditions = Vec::new();
    for value in values(graph, node, "condition")? {
        if value == Term::from(iri(RDF, "nil"))
            || !objects(graph, &value, &iri(RDF, "first"))?.is_empty()
        {
            conditions.extend(rdf_list(graph, &value)?);
        } else {
            conditions.push(value);
        }
    }
    Ok(model::ShaclRule {
        id: node.clone(),
        order: one(graph, node, "order")?,
        deactivated: boolean(graph, node, "deactivated")?,
        conditions,
        kinds,
    })
}

pub(crate) fn prefix_resource(
    graph: &(impl GraphRead + ?Sized),
    node: Term,
) -> Result<model::PrefixDeclarations<Term>> {
    let mut declarations = Vec::new();
    for id in values(graph, &node, "declare")? {
        let (Some(prefix), Some(namespace)) =
            (one(graph, &id, "prefix")?, one(graph, &id, "namespace")?)
        else {
            return Err(Error::Invalid(
                "Prefix declaration requires sh:prefix and sh:namespace literals".into(),
            ));
        };
        declarations.push(model::PrefixDeclaration {
            id,
            prefix,
            namespace,
        });
    }
    Ok(model::PrefixDeclarations {
        imports: objects(
            graph,
            &node,
            &iri("http://www.w3.org/2002/07/owl#", "imports"),
        )?,
        id: node,
        declarations,
    })
}
