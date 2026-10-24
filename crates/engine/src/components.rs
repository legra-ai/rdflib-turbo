use std::collections::HashSet;

use crate::model;
use oxigraph::model::*;

use crate::constraints::{Constraint, named};
use crate::graph::*;
use crate::path::Path;
use crate::{Error, GraphRead, Result};

#[cfg(test)]
#[path = "components_tests.rs"]
mod tests;

pub(crate) struct Parameter {
    pub declaration: model::Parameter<Term>,
    variable: Variable,
}

pub(crate) struct Component {
    pub node: NamedNode,
    pub parameters: Vec<Parameter>,
    pub validators: Vec<Term>,
    node_validator: Option<Term>,
    property_validator: Option<Term>,
    validator: Option<Term>,
}

impl Component {
    pub fn discover(graph: &(impl GraphRead + ?Sized)) -> Result<Vec<Self>> {
        let mut result = Vec::new();
        for node in instances(graph, &iri(SH, "ConstraintComponent").into())? {
            // Built-in declarations are vocabulary metadata. Their constraints
            // are compiled by the native Core/AF implementation, not registered
            // a second time as custom components requiring a SPARQL validator.
            const BUILTIN: &[&str] = &[
                "AndConstraintComponent",
                "ClassConstraintComponent",
                "ClosedConstraintComponent",
                "DatatypeConstraintComponent",
                "DisjointConstraintComponent",
                "EqualsConstraintComponent",
                "ExpressionConstraintComponent",
                "HasValueConstraintComponent",
                "InConstraintComponent",
                "LanguageInConstraintComponent",
                "LessThanConstraintComponent",
                "LessThanOrEqualsConstraintComponent",
                "MaxCountConstraintComponent",
                "MaxExclusiveConstraintComponent",
                "MaxInclusiveConstraintComponent",
                "MaxLengthConstraintComponent",
                "MinCountConstraintComponent",
                "MinExclusiveConstraintComponent",
                "MinInclusiveConstraintComponent",
                "MinLengthConstraintComponent",
                "NodeConstraintComponent",
                "NodeKindConstraintComponent",
                "NotConstraintComponent",
                "OrConstraintComponent",
                "PatternConstraintComponent",
                "SingleLineConstraintComponent",
                "PropertyConstraintComponent",
                "QualifiedMaxCountConstraintComponent",
                "QualifiedMinCountConstraintComponent",
                "QualifiedValueShapeConstraintComponent",
                "SPARQLConstraintComponent",
                "UniqueLangConstraintComponent",
                "XoneConstraintComponent",
                "sourceConstraintComponent",
            ];
            if matches!(&node, Term::NamedNode(n) if n.as_str().strip_prefix(SH).is_some_and(|name| BUILTIN.contains(&name)))
            {
                continue;
            }
            let mut parameters = Vec::new();
            let mut names = HashSet::new();
            for parameter in values(graph, &node, "parameter")? {
                let path = named(
                    one(graph, &parameter, "path")?
                        .ok_or_else(|| Error::Invalid("Parameter needs sh:path".into()))?,
                    "Parameter sh:path",
                )?;
                let name = path.as_str().rsplit(['#', '/']).next().unwrap_or("");
                let variable = Variable::new(name)
                    .map_err(|e| Error::Invalid(format!("Invalid parameter name: {e}")))?;
                if [
                    "this",
                    "value",
                    "path",
                    "PATH",
                    "currentShape",
                    "shapesGraph",
                ]
                .contains(&name)
                    || !names.insert(variable.clone())
                {
                    return Err(Error::Invalid(format!(
                        "Invalid or duplicate parameter name {name}"
                    )));
                }
                let mut declaration = model::Parameter::new(
                    parameter.clone(),
                    crate::structural::named_role(path.into(), "Parameter path")?,
                );
                declaration.optional = boolean(graph, &parameter, "optional")?;
                parameters.push(Parameter {
                    declaration,
                    variable,
                });
            }
            if parameters.is_empty() || parameters.iter().all(|p| p.declaration.optional) {
                return Err(Error::Invalid(
                    "Constraint component needs a required parameter".into(),
                ));
            }
            let node_validator = one(graph, &node, "nodeValidator")?;
            let property_validator = one(graph, &node, "propertyValidator")?;
            let validator = one(graph, &node, "validator")?;
            let validators = [&node_validator, &property_validator, &validator]
                .into_iter()
                .flatten()
                .cloned()
                .collect();
            result.push(Self {
                node: named(node, "Constraint component")?,
                parameters,
                validators,
                node_validator,
                property_validator,
                validator,
            });
        }
        Ok(result)
    }

    pub fn read_application(
        &self,
        graph: &(impl GraphRead + ?Sized),
        shape: &Term,
    ) -> Result<Option<model::CustomConstraint<Term>>> {
        let mut parameters = Vec::new();
        for parameter in &self.parameters {
            let p = &parameter.declaration;
            let values = objects(graph, shape, &p.path.clone())?;
            if values.is_empty() && !p.optional {
                return Ok(None);
            }
            parameters.push(model::ParameterValues {
                path: p.path.clone(),
                values,
            });
        }
        Ok(Some(model::CustomConstraint {
            component: crate::structural::named_role(
                self.node.clone().into(),
                "Constraint component",
            )?,
            parameters,
        }))
    }

    pub fn lower(
        &self,
        graph: &(impl GraphRead + ?Sized),
        application: model::CustomConstraint<Term>,
        path: Option<&Path>,
        namespaces: &[(String, String)],
    ) -> Result<Vec<Constraint>> {
        let mut combinations = vec![Vec::new()];
        for parameter in &self.parameters {
            let values = &application
                .parameters
                .iter()
                .find(|p| p.path == parameter.declaration.path)
                .ok_or_else(|| {
                    Error::Invalid(format!(
                        "Missing parameter binding {}",
                        parameter.declaration.path
                    ))
                })?
                .values;
            if values.is_empty() {
                if parameter.declaration.optional {
                    continue;
                }
                return Err(Error::Invalid(
                    "Active constraint component requires its parameter values".into(),
                ));
            }
            let mut next = Vec::new();
            for combination in combinations {
                for value in values {
                    let mut bindings = combination.clone();
                    bindings.push((parameter.variable.clone(), value.clone()));
                    next.push(bindings);
                }
            }
            combinations = next;
        }
        let select_validator = if path.is_some() {
            self.property_validator.as_ref()
        } else {
            self.node_validator.as_ref()
        };
        let (validator, ask) = if let Some(validator) = select_validator {
            (validator, false)
        } else if let Some(validator) = &self.validator {
            (validator, true)
        } else {
            return Err(Error::Unsupported(format!(
                "No applicable validator for {}",
                self.node
            )));
        };
        // Resolve only the active validator role. Unused validator resources
        // may be incomplete metadata and must not activate query parsing.
        let declaration = crate::structural::validator(graph, validator, ask)?;
        let deactivated = boolean(graph, validator, "deactivated")?;
        combinations
            .iter()
            .map(|bindings| {
                crate::sparql::lower_validator(
                    graph,
                    declaration.clone(),
                    path,
                    bindings,
                    deactivated,
                    Some(self.node.clone()),
                    namespaces,
                )
                .map(|query| Constraint::Sparql(Box::new(query)))
            })
            .collect()
    }
}
