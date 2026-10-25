use super::*;

impl CompiledShapes {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate_relational_constraint(
        &self,
        index: usize,
        focus: &Term,
        values: &[Term],
        data: &(impl GraphRead + ?Sized),
        constraint: &Constraint,
        results: &mut Vec<Violation>,
        evaluation: &mut Evaluation,
    ) -> Result<()> {
        let shape = &self.shapes[index];
        let violation =
            |component, value: Option<&Term>| Violation::new(index, shape, focus, component, value);
        match constraint {
            Constraint::Pair {
                kind,
                predicate,
                query,
                operator,
            } => {
                let others = objects(data, focus, predicate)?;
                if let Some(query) = query {
                    for value in values {
                        for other in &others {
                            if !crate::value::compare(
                                &evaluation.comparison,
                                crate::ComparisonRole::PropertyPair,
                                operator.expect("Ordering pair has operator"),
                                query,
                                data,
                                value,
                                other,
                            )? {
                                results.push(violation(kind, Some(value)).with_comparison(
                                    ComparisonMessage::PropertyPair {
                                        operator: operator.expect("Ordering pair has operator"),
                                        predicate: predicate.clone(),
                                    },
                                ));
                            }
                        }
                    }
                } else if *kind == "EqualsConstraintComponent" {
                    for value in values
                        .iter()
                        .filter(|v| !others.contains(v))
                        .chain(others.iter().filter(|v| !values.contains(v)))
                    {
                        results.push(violation(kind, Some(value)));
                    }
                } else {
                    for value in values.iter().filter(|v| others.contains(v)) {
                        results.push(violation(kind, Some(value)));
                    }
                }
            }
            Constraint::Node(target) => {
                if !values.is_empty()
                    && evaluation.skip_recursive(
                        index,
                        "NodeConstraintComponent",
                        *target,
                        &self.shapes,
                    )
                {
                    return Ok(());
                }
                for value in values {
                    let mut details = Vec::new();
                    self.evaluate_child(
                        index,
                        "NodeConstraintComponent",
                        *target,
                        value,
                        data,
                        &mut details,
                        evaluation,
                    )?;
                    if !details.is_empty() {
                        let mut parent = violation("NodeConstraintComponent", Some(value));
                        parent.details = details;
                        results.push(parent);
                    }
                }
            }
            Constraint::Logical { kind, shapes } => {
                // Only sh:not has the reference warn-and-skip policy.
                // and/or/xone recursion is bounded by max_validation_depth.
                if *kind == "NotConstraintComponent"
                    && shapes
                        .iter()
                        .any(|&target| evaluation.skip_recursive(index, kind, target, &self.shapes))
                {
                    return Ok(());
                }
                for value in values {
                    let mut count = 0;
                    for &target in shapes {
                        let mut nested = Vec::new();
                        self.evaluate_child(
                            index,
                            kind,
                            target,
                            value,
                            data,
                            &mut nested,
                            evaluation,
                        )?;
                        count += usize::from(nested.is_empty());
                    }
                    let valid = match *kind {
                        "NotConstraintComponent" => count == 0,
                        "AndConstraintComponent" => count == shapes.len(),
                        "OrConstraintComponent" => count > 0,
                        "XoneConstraintComponent" => count == 1,
                        _ => unreachable!(),
                    };
                    if !valid {
                        results.push(violation(kind, Some(value)));
                    }
                }
            }
            Constraint::Qualified {
                shape: target,
                minimum,
                maximum,
                disjoint,
                siblings,
            } => {
                let has_values = evaluation
                    .batch
                    .filter(|batch| batch.shape == index && batch.depth == evaluation.path.len())
                    .map_or(!values.is_empty(), |batch| batch.has_values);
                if !has_values
                    && !disjoint
                    && minimum.as_ref().is_none_or(|min| min.allows(0, true))
                {
                    return Ok(());
                }
                if evaluation.skip_recursive(
                    index,
                    "QualifiedValueShapeConstraintComponent",
                    *target,
                    &self.shapes,
                ) {
                    return Ok(());
                }
                let mut count = 0usize;
                for value in values {
                    let mut nested = Vec::new();
                    self.evaluate_child(
                        index,
                        "QualifiedValueShapeConstraintComponent",
                        *target,
                        value,
                        data,
                        &mut nested,
                        evaluation,
                    )?;
                    if !nested.is_empty() {
                        continue;
                    }
                    let mut disjoint = true;
                    for &sibling in siblings {
                        let mut sibling_results = Vec::new();
                        self.evaluate_child(
                            index,
                            "QualifiedValueShapeConstraintComponent",
                            sibling,
                            value,
                            data,
                            &mut sibling_results,
                            evaluation,
                        )?;
                        if sibling_results.is_empty() {
                            disjoint = false;
                            break;
                        }
                    }
                    count += usize::from(disjoint);
                }
                if minimum.as_ref().is_some_and(|min| !min.allows(count, true)) {
                    results.push(violation("QualifiedMinCountConstraintComponent", None));
                }
                if maximum
                    .as_ref()
                    .is_some_and(|max| !max.allows(count, false))
                {
                    results.push(violation("QualifiedMaxCountConstraintComponent", None));
                }
            }
            Constraint::Closed(allowed) => {
                for value in values {
                    for quad in data.matching_quads(
                        Some(value.as_ref()),
                        None,
                        None,
                        Some(GraphNameRef::DefaultGraph),
                    ) {
                        let quad = quad?;
                        if quad.predicate == Term::from(iri(RDF, "type"))
                            && quad.object == Term::from(iri(RDFS, "Resource"))
                        {
                            continue;
                        }
                        let permitted = matches!(&quad.predicate, Term::NamedNode(node) if allowed.contains(node));
                        if !permitted {
                            let mut result =
                                violation("ClosedConstraintComponent", Some(&quad.object));
                            result.path = Some(quad.predicate);
                            results.push(result);
                        }
                    }
                }
            }
            _ => unreachable!("constraint dispatched to its compiled category"),
        }
        Ok(())
    }
}
