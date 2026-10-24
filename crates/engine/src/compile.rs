use std::collections::{HashMap, HashSet};

use crate::expression::NodeExpression;
use crate::functions::{Functions, NativeQuery};
use bigdecimal::BigDecimal;
use oxigraph::model::*;
use spargebra::algebra::GraphPattern;
use spargebra::{Query, SparqlParser};

use crate::constraints::{self, Constraint};
use crate::graph::*;
use crate::model;
use crate::path::Path;
use crate::{Error, GraphRead, IndexedShapesSource, Result};

#[cfg(test)]
#[path = "compile_tests.rs"]
mod tests;

pub(crate) enum Target {
    Node(Term),
    Class(Term),
    Subjects(NamedNode),
    Objects(NamedNode),
    Sparql {
        query: Box<NativeQuery>,
        bindings: Vec<(Variable, Term)>,
    },
}

pub(crate) struct Rule {
    pub node: Term,
    pub order: BigDecimal,
    pub deactivated: bool,
    pub conditions: Vec<usize>,
    pub queries: Vec<NativeQuery>,
    pub triple: Option<[NodeExpression; 3]>,
}

pub(crate) struct Shape {
    pub node: Term,
    pub path: Option<Path>,
    pub path_node: Option<Term>,
    pub order: BigDecimal,
    pub deactivated: bool,
    pub targets: Vec<Target>,
    pub constraints: Vec<Constraint>,
    pub properties: Vec<usize>,
    pub rules: Vec<Rule>,
    pub messages: Vec<Term>,
    pub severity: NamedNode,
}

/// Immutable compiled shapes and SPARQL queries; safe to share across validations.
pub struct CompiledShapes {
    pub(crate) graph: IndexedShapesSource,
    pub(crate) shapes: Vec<Shape>,
    pub(crate) execution_order: Vec<usize>,
    pub(crate) functions: Functions,
    pub(crate) namespaces: Vec<(String, String)>,
}

fn order(value: Option<Term>, node: &Term) -> Result<BigDecimal> {
    match value {
        None => Ok(BigDecimal::from(0)),
        Some(Term::Literal(v))
            if ["integer", "decimal", "float", "double"]
                .iter()
                .any(|t| v.datatype() == iri(XSD, t).as_ref()) =>
        {
            v.value()
                .parse()
                .map_err(|_| Error::Invalid(format!("Invalid sh:order on {node}")))
        }
        _ => Err(Error::Invalid(format!(
            "sh:order must be numeric on {node}"
        ))),
    }
}

fn query(
    store: &(impl GraphRead + ?Sized),
    declaration: model::SparqlQuery<Term>,
    construct: bool,
    namespaces: &[(String, String)],
) -> Result<NativeQuery> {
    let Term::Literal(text) = declaration.text else {
        return Err(Error::Invalid(
            "SPARQL query must be a string literal".into(),
        ));
    };
    if text.datatype() != iri(XSD, "string").as_ref() {
        return Err(Error::Invalid("SPARQL query must be an xsd:string".into()));
    }
    let mut parsed = SparqlParser::new()
        .parse_query(&format!(
            "{}\n{}",
            crate::sparql::prefixes(store, declaration.prefixes, namespaces)?,
            text.value()
        ))
        .map_err(|e| Error::Invalid(format!("SPARQL parse error: {e}")))?;
    if (construct && !matches!(parsed, Query::Construct { .. }))
        || (!construct && !matches!(parsed, Query::Select { .. }))
    {
        return Err(Error::Invalid(format!(
            "SPARQL query must be {}",
            if construct { "CONSTRUCT" } else { "SELECT" }
        )));
    }
    match &mut parsed {
        Query::Select { pattern, .. } => {
            let mut has_this = false;
            pattern.on_in_scope_variable(|v| has_this |= v.as_str() == "this");
            if !has_this {
                return Err(Error::Invalid("SPARQL target must project ?this".into()));
            }
        }
        Query::Construct { pattern, .. } => {
            // Oxigraph substitutions require a projected variable. Make the
            // SHACL prebinding explicit in the algebra, even when $this occurs
            // only in a filter/template or is absent from a constant rule.
            let mut variables = HashSet::new();
            pattern.on_in_scope_variable(|v| {
                variables.insert(v.clone());
            });
            variables.insert(Variable::new_unchecked("this"));
            *pattern = GraphPattern::Project {
                inner: Box::new(pattern.clone()),
                variables: variables.into_iter().collect(),
            };
        }
        _ => unreachable!("Query kind was checked above"),
    }
    Ok(NativeQuery::new(parsed))
}

fn lower_target(
    graph: &(impl GraphRead + ?Sized),
    target: model::Target<Term>,
    namespaces: &[(String, String)],
) -> Result<Target> {
    Ok(match target {
        model::Target::Node(node) => Target::Node(node),
        model::Target::Class(class) => Target::Class(class.into()),
        model::Target::ImplicitClass(class) => Target::Class(class),
        model::Target::SubjectsOf(predicate) => Target::Subjects(predicate),
        model::Target::ObjectsOf(predicate) => Target::Objects(predicate),
        model::Target::Sparql(target) => Target::Sparql {
            query: Box::new(query(graph, target.select, false, namespaces)?),
            bindings: Vec::new(),
        },
        model::Target::Custom(target) => {
            let definition = crate::structural::target_type(graph, &target.target_type)?;
            let mut bindings = Vec::new();
            for parameter in crate::functions::lower_parameters(definition.parameters)? {
                let values = &target
                    .parameters
                    .iter()
                    .find(|p| p.path.as_str() == parameter.path.as_str())
                    .ok_or_else(|| {
                        Error::Invalid(format!("Missing target parameter {}", parameter.path))
                    })?
                    .values;
                if values.len() > 1 || (values.is_empty() && !parameter.optional) {
                    return Err(Error::Invalid(format!(
                        "Target {} requires one value for {}",
                        target.id, parameter.path
                    )));
                }
                if let Some(value) = values.first() {
                    bindings.push((parameter.variable, value.clone()));
                }
            }
            let mut parsed = query(graph, definition.select, false, namespaces)?.parsed;
            crate::functions::project_bindings(
                &mut parsed,
                &bindings.iter().map(|(v, _)| v.clone()).collect::<Vec<_>>(),
            );
            Target::Sparql {
                query: Box::new(NativeQuery::new(parsed)),
                bindings,
            }
        }
    })
}

fn lower_rule(
    graph: &(impl GraphRead + ?Sized),
    declaration: model::ShaclRule<Term>,
    indices: &HashMap<Term, usize>,
    namespaces: &[(String, String)],
) -> Result<Rule> {
    let mut queries = Vec::new();
    let mut triple = None;
    for kind in declaration.kinds {
        match kind {
            model::RuleKind::Sparql(body) => queries.push(query(graph, body, true, namespaces)?),
            model::RuleKind::Triple(body) => {
                if triple.is_some() {
                    return Err(Error::Invalid("Multiple TripleRule bodies".into()));
                }
                triple = Some([
                    NodeExpression::lower(body.subject, indices)?,
                    NodeExpression::lower(body.predicate, indices)?,
                    NodeExpression::lower(body.object, indices)?,
                ]);
            }
        }
    }
    Ok(Rule {
        order: order(declaration.order, &declaration.id)?,
        node: declaration.id,
        deactivated: declaration.deactivated,
        conditions: declaration
            .conditions
            .iter()
            .map(|n| {
                indices
                    .get(n)
                    .copied()
                    .ok_or_else(|| Error::Invalid("Unknown condition shape".into()))
            })
            .collect::<Result<_>>()?,
        queries,
        triple,
    })
}

fn check_vocabulary(
    store: &(impl GraphRead + ?Sized),
    advanced: bool,
    executable_nodes: &HashSet<Term>,
) -> Result<()> {
    // Deliberately limited to supported features. This also catches
    // misspelled SHACL terms instead of silently treating them as annotations.
    const ALLOWED: &[&str] = &[
        "targetNode",
        "targetClass",
        "targetSubjectsOf",
        "targetObjectsOf",
        "target",
        "select",
        "prefixes",
        "property",
        "path",
        "minCount",
        "maxCount",
        "hasValue",
        "message",
        "severity",
        "deactivated",
        "order",
        "rule",
        "condition",
        "construct",
        "name",
        "description",
        // The reference validator ignores this newer vocabulary annotation.
        // Recognizing its name does not enable the typed SingleLine component.
        "singleLine",
        "group",
        "defaultValue",
        "nodeKind",
        "optional",
        "parameter",
        "validator",
        "nodeValidator",
        "propertyValidator",
        "ask",
        "labelTemplate",
        "label",
        "keyParameter",
        "returnType",
        "subject",
        "predicate",
        "object",
        "expression",
        "filterShape",
        "nodes",
        "union",
        "intersection",
    ];
    for quad in store.matching_quads(None, None, None, Some(GraphNameRef::DefaultGraph)) {
        let quad = quad?;
        // Function/component declarations have implicit global registration.
        if advanced
            && quad.predicate == Term::from(iri(RDF, "type"))
            && let Term::NamedNode(ref n) = quad.object
            && let Some(name) = n.as_str().strip_prefix(SH)
            && ["Function", "SHACLFunction", "JSFunction"].contains(&name)
        {
            return Err(Error::Unsupported(format!("sh:{name}")));
        }
        // A shapes graph may also contain ordinary data and prior reports.
        // Only executable resources determine the supported SHACL program.
        if !executable_nodes.contains(&quad.subject) {
            continue;
        }
        if let Term::NamedNode(predicate) = &quad.predicate
            && let Some(name) = predicate.as_str().strip_prefix(SH)
            && !ALLOWED.contains(&name)
            && !constraints::PARAMETERS.contains(&name)
        {
            return Err(Error::Unsupported(format!("sh:{name} on {}", quad.subject)));
        }
        if quad.predicate == Term::from(iri(RDF, "type"))
            && let Term::NamedNode(ref n) = quad.object
            && let Some(name) = n.as_str().strip_prefix(SH)
            && ![
                "NodeShape",
                "PropertyShape",
                "SPARQLRule",
                "SPARQLTarget",
                "SPARQLConstraint",
                "ConstraintComponent",
                "Parameter",
                "SPARQLAskValidator",
                "SPARQLSelectValidator",
                "SPARQLFunction",
                "TripleRule",
                "SPARQLTargetType",
                "TargetType",
                "SPARQLConstraintObject",
            ]
            .contains(&name)
        {
            return Err(Error::Unsupported(format!("sh:{name}")));
        }
    }
    Ok(())
}

impl CompiledShapes {
    /// Shape identities discovered by the shared structural compiler.
    pub fn shape_nodes(&self) -> impl Iterator<Item = &Term> {
        self.shapes.iter().map(|shape| &shape.node)
    }

    pub fn compile(store: IndexedShapesSource, advanced: bool) -> Result<Self> {
        Self::compile_with_boolean_policy(store, advanced, true)
    }

    pub fn compile_with_boolean_policy(
        store: IndexedShapesSource,
        advanced: bool,
        numeric_booleans: bool,
    ) -> Result<Self> {
        Self::compile_with_namespaces(store, advanced, numeric_booleans, &[])
    }

    pub fn compile_with_namespaces(
        source: IndexedShapesSource,
        advanced: bool,
        numeric_booleans: bool,
        namespaces: &[(String, String)],
    ) -> Result<Self> {
        Self::compile_with_decimal_policy(
            source,
            advanced,
            numeric_booleans,
            namespaces,
            constraints::decimal_digit,
        )
    }

    /// Compile with the caller's immutable Unicode decimal scalar table.
    ///
    /// ASCII digits use the built-in fast path. A caller's `None` for every
    /// other scalar is authoritative; no alternate Unicode table is consulted.
    pub fn compile_with_decimal_policy(
        source: IndexedShapesSource,
        advanced: bool,
        numeric_booleans: bool,
        namespaces: &[(String, String)],
        non_ascii_decimal: fn(char) -> Option<u8>,
    ) -> Result<Self> {
        Self::compile_with_regex_policy(
            source,
            advanced,
            numeric_booleans,
            namespaces,
            non_ascii_decimal,
            None,
        )
    }

    /// Compile using explicit host scalar and lexical regex authorities.
    /// The lexical resolver is borrowed during compilation only; matching
    /// retains immutable native tables and never invokes the host interpreter.
    pub fn compile_with_host_policy(
        source: IndexedShapesSource,
        advanced: bool,
        numeric_booleans: bool,
        namespaces: &[(String, String)],
        non_ascii_decimal: fn(char) -> Option<u8>,
        policy: std::sync::Arc<crate::RegexPolicy>,
        lexical: &dyn crate::RegexLexicalResolver,
    ) -> Result<Self> {
        Self::compile_with_regex_policy(
            source,
            advanced,
            numeric_booleans,
            namespaces,
            non_ascii_decimal,
            Some((policy, lexical)),
        )
    }

    fn compile_with_regex_policy(
        source: IndexedShapesSource,
        advanced: bool,
        numeric_booleans: bool,
        namespaces: &[(String, String)],
        non_ascii_decimal: fn(char) -> Option<u8>,
        regex: Option<(
            std::sync::Arc<crate::RegexPolicy>,
            &dyn crate::RegexLexicalResolver,
        )>,
    ) -> Result<Self> {
        let digits = constraints::DecimalDigitPolicy::new(non_ascii_decimal);
        let store = crate::shapes_policy::ShapesPolicy::new(&source, numeric_booleans);
        let components = crate::components::Component::discover(&store)?;
        let functions = if advanced {
            Functions::compile(&store, namespaces)?
        } else {
            Functions::default()
        };
        let mut nodes = HashSet::new();
        for kind in ["NodeShape", "PropertyShape"] {
            nodes.extend(subjects(
                &store,
                &iri(RDF, "type"),
                Some(&iri(SH, kind).into()),
            )?);
        }
        for name in [
            "targetNode",
            "targetClass",
            "targetSubjectsOf",
            "targetObjectsOf",
        ] {
            nodes.extend(subjects(&store, &iri(SH, name), None)?);
        }
        if advanced {
            nodes.extend(subjects(&store, &iri(SH, "rule"), None)?);
        }
        let mut parameters = instances(&store, &iri(SH, "Parameter").into())?;
        parameters.extend(
            store
                .matching_quads(
                    None,
                    Some(iri(SH, "parameter").as_ref().into()),
                    None,
                    Some(GraphNameRef::DefaultGraph),
                )
                .map(|q| q.map(|q| q.object))
                .collect::<Result<Vec<_>>>()?,
        );
        for name in ["property", "node"] {
            // Parameter declarations can describe their argument with sh:node
            // or sh:property. This metadata is not a validation entry point.
            // An explicit shape type, target or shape reference still gives a
            // parameter a second, executable role.
            nodes.extend(
                subjects(&store, &iri(SH, name), None)?
                    .into_iter()
                    .filter(|n| !parameters.contains(n)),
            );
        }
        nodes.extend(constraints::shape_references(&store, advanced)?);
        if advanced {
            for quad in store.matching_quads(
                None,
                Some(iri(SH, "filterShape").as_ref().into()),
                None,
                Some(GraphNameRef::DefaultGraph),
            ) {
                nodes.insert(quad?.object);
            }
        }
        // sh:path can annotate parameter or expression metadata. Decode a
        // property path only when its subject has an executable shape role.
        let mut paths = HashMap::new();
        for shape in &nodes {
            for node in values(&store, shape, "path")? {
                if let std::collections::hash_map::Entry::Vacant(entry) = paths.entry(node.clone())
                {
                    entry.insert(Path::read(&store, &node)?);
                }
            }
        }
        let mut executable_nodes = nodes.clone();
        for node in &nodes {
            executable_nodes.extend(values(&store, node, "sparql")?);
            if advanced {
                executable_nodes.extend(values(&store, node, "target")?);
                executable_nodes.extend(values(&store, node, "rule")?);
            }
        }
        for component in &components {
            executable_nodes.insert(component.node.clone().into());
            executable_nodes.extend(
                component
                    .parameters
                    .iter()
                    .map(|p| p.declaration.id.clone()),
            );
            executable_nodes.extend(component.validators.iter().cloned());
        }
        // Shape roles come from types, targets and shape-expecting predicates.
        // Constraint parameter annotations alone do not turn metadata into an
        // executable shape (e.g. sh:path on a function/component parameter).
        executable_nodes.extend(nodes.iter().cloned());
        check_vocabulary(&store, advanced, &executable_nodes)?;
        let mut nodes: Vec<_> = nodes.into_iter().collect();
        nodes.sort_by_key(ToString::to_string);
        let indices: HashMap<_, _> = nodes.iter().cloned().zip(0..).collect();
        let mut shapes = Vec::new();
        for node in nodes {
            let declaration = crate::structural::read_shape(
                &store,
                &node,
                &paths,
                advanced,
                &functions,
                &components,
                digits,
            )?;
            let (declaration, path, path_node) = match declaration {
                crate::structural::ShapeDeclaration::Node(shape) => (*shape, None, None),
                crate::structural::ShapeDeclaration::Property(shape) => (
                    model::NodeShape {
                        id: shape.id,
                        targets: shape.targets,
                        components: shape.components,
                        property_shapes: shape.property_shapes,
                        deactivated: shape.deactivated,
                        severity: shape.severity,
                        messages: shape.messages,
                        rules: shape.rules,
                        order: shape.order,
                    },
                    Some(Path::from(shape.path)),
                    shape.path_node,
                ),
            };
            let targets = declaration
                .targets
                .into_iter()
                .map(|target| lower_target(&store, target, namespaces))
                .collect::<Result<Vec<_>>>()?;
            let mut constraints = Vec::new();
            for component in declaration.components {
                match component {
                    model::Component::Sparql(sparql) => {
                        constraints.push(Constraint::Sparql(Box::new(crate::sparql::lower(
                            &store,
                            *sparql,
                            path.as_ref(),
                            namespaces,
                        )?)))
                    }
                    model::Component::Expression(expression) => {
                        constraints.push(Constraint::NodeExpression {
                            node: expression.expression.node.clone(),
                            expression: NodeExpression::lower(expression.expression, &indices)?,
                            messages: expression.messages,
                        })
                    }
                    model::Component::Custom(application) => {
                        let definition = components
                            .iter()
                            .find(|c| c.node.as_str() == application.component.as_str())
                            .ok_or_else(|| {
                                Error::Invalid(format!(
                                    "Unknown constraint component {}",
                                    application.component
                                ))
                            })?;
                        constraints.extend(definition.lower(
                            &store,
                            application,
                            path.as_ref(),
                            namespaces,
                        )?);
                    }
                    core => {
                        if let Some(constraint) = constraints::lower_one(
                            &store,
                            &declaration.id,
                            path.is_some(),
                            &indices,
                            core,
                            digits,
                            regex.as_ref().map(|(policy, lexical)| (policy, *lexical)),
                        )? {
                            constraints.push(constraint);
                        }
                    }
                }
            }
            let properties = declaration
                .property_shapes
                .iter()
                .map(|n| {
                    indices
                        .get(n)
                        .copied()
                        .ok_or_else(|| Error::Invalid("Unknown property shape".into()))
                })
                .collect::<Result<_>>()?;
            let mut rules = declaration
                .rules
                .into_iter()
                .map(|rule| lower_rule(&store, rule, &indices, namespaces))
                .collect::<Result<Vec<_>>>()?;
            rules.sort_by(|a, b| {
                a.order
                    .cmp(&b.order)
                    .then_with(|| a.node.to_string().cmp(&b.node.to_string()))
            });
            shapes.push(Shape {
                order: order(declaration.order, &declaration.id)?,
                deactivated: declaration.deactivated,
                node: declaration.id,
                path,
                path_node,
                targets,
                constraints,
                properties,
                rules,
                messages: declaration.messages,
                severity: NamedNode::new_unchecked(declaration.severity.as_iri()),
            });
        }
        for shape in &shapes {
            for &p in &shape.properties {
                if shapes[p].path.is_none() {
                    return Err(Error::Invalid(format!(
                        "sh:property points to {} without sh:path",
                        shapes[p].node
                    )));
                }
            }
        }
        let mut execution_order: Vec<_> = (0..shapes.len()).collect();
        execution_order.sort_by(|&a, &b| shapes[a].order.cmp(&shapes[b].order));
        Ok(Self {
            graph: source,
            shapes,
            execution_order,
            functions,
            namespaces: namespaces.to_vec(),
        })
    }
}
