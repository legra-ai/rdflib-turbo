use super::*;

impl CompiledShapes {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate_advanced_constraint(
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
        let functions = evaluation.functions.clone();
        let violation =
            |component, value: Option<&Term>| Violation::new(index, shape, focus, component, value);
        match constraint {
            Constraint::NodeExpression {
                expression,
                node,
                messages,
            } => {
                for value in values {
                    let nodes = expression.evaluate(value, data, &functions, &mut |i, n| {
                        let mut nested = Vec::new();
                        self.evaluate_child(
                            index,
                            "ExpressionConstraintComponent",
                            i,
                            n,
                            data,
                            &mut nested,
                            evaluation,
                        )?;
                        Ok(nested.is_empty())
                    })?;
                    if nodes != HashSet::from([Literal::from(true).into()]) {
                        let mut result = violation("ExpressionConstraintComponent", Some(value));
                        result.source_constraint = Some(node.clone());
                        if !messages.is_empty() {
                            result.messages = Some(messages.clone());
                        }
                        results.push(result);
                    }
                }
            }
            Constraint::Sparql(query) => {
                let inputs = if query.per_value {
                    values.iter().map(Some).collect()
                } else {
                    vec![None]
                };
                for input in inputs {
                    for row in query.evaluate(
                        data,
                        &self.graph,
                        &shape.node,
                        focus,
                        &shape.messages,
                        input,
                        &functions,
                    )? {
                        let mut result = violation("SPARQLConstraintComponent", row.value.as_ref());
                        result.focus = row.focus;
                        if result.value.is_none() && shape.path.is_none() {
                            result.value = Some(focus.clone());
                        }
                        if query.per_value && shape.path.is_some() {
                            // Match pySHACL's optional sh:value omission for
                            // ASK validators attached to property shapes.
                            result.value = None;
                        }
                        if row.path.is_some() {
                            result.path = row.path;
                        }
                        if let Some(component) = &query.component {
                            result.component = component.clone();
                        } else {
                            result.source_constraint = Some(query.node.clone());
                        }
                        if !row.messages.is_empty() {
                            result.messages = Some(row.messages);
                        }
                        results.push(result);
                    }
                }
            }
            _ => unreachable!("constraint dispatched to its compiled category"),
        }
        Ok(())
    }
}
