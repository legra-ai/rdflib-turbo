use super::*;

impl CompiledShapes {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate_child(
        &self,
        parent: usize,
        component: &'static str,
        index: usize,
        focus: &Term,
        data: &(impl GraphRead + ?Sized),
        results: &mut Vec<Violation>,
        evaluation: &mut Evaluation,
    ) -> Result<()> {
        evaluation.path.push(EvaluationStep {
            shape: parent,
            component,
        });
        let result = self.evaluate_batch(
            index,
            std::slice::from_ref(focus),
            data,
            results,
            evaluation,
        );
        evaluation.path.pop();
        result
    }

    pub(super) fn evaluate_batch(
        &self,
        index: usize,
        focus: &[Term],
        data: &(impl GraphRead + ?Sized),
        results: &mut Vec<Violation>,
        evaluation: &mut Evaluation,
    ) -> Result<()> {
        let shape = &self.shapes[index];
        let batch = if focus.len() > 1
            && !shape.deactivated
            && shape
                .constraints
                .iter()
                .any(|constraint| matches!(constraint, Constraint::Qualified { .. }))
        {
            // pySHACL's qualified-count empty-input shortcut applies to the
            // whole focus batch. Nested property validations receive one
            // focus each, so their scopes must not inherit this batch.
            let mut has_values = shape.path.is_none();
            if let Some(path) = &shape.path {
                for node in focus {
                    if !evaluation.values(path, data, node)?.is_empty() {
                        has_values = true;
                        break;
                    }
                }
            }
            Some(EvaluationBatch {
                shape: index,
                depth: evaluation.path.len(),
                has_values,
            })
        } else {
            None
        };
        let previous = std::mem::replace(&mut evaluation.batch, batch);
        let result = focus
            .iter()
            .try_for_each(|node| self.evaluate_inner(index, node, data, results, evaluation));
        evaluation.batch = previous;
        result
    }

    pub(super) fn evaluate_inner(
        &self,
        index: usize,
        focus: &Term,
        data: &(impl GraphRead + ?Sized),
        results: &mut Vec<Violation>,
        evaluation: &mut Evaluation,
    ) -> Result<()> {
        let shape = &self.shapes[index];
        if shape.deactivated {
            return Ok(());
        }
        if evaluation.path.len() >= evaluation.max_depth {
            return Err(Error::Runtime(format!(
                "Validation path too deep!\n{}",
                evaluation.describe_path(&self.shapes, None)
            )));
        }
        let values = if let Some(path) = &shape.path {
            evaluation.values(path, data, focus)?
        } else {
            vec![focus.clone()]
        };
        for constraint in &shape.constraints {
            match constraint {
                Constraint::NodeExpression { .. } | Constraint::Sparql(_) => self
                    .evaluate_advanced_constraint(
                        index, focus, &values, data, constraint, results, evaluation,
                    )?,
                Constraint::Count { .. }
                | Constraint::HasValue(_)
                | Constraint::Class(_)
                | Constraint::Datatype(_)
                | Constraint::NodeKind(_)
                | Constraint::Pattern(_)
                | Constraint::Expression { .. }
                | Constraint::Length { .. }
                | Constraint::LanguageIn(_)
                | Constraint::UniqueLang
                | Constraint::SingleLine
                | Constraint::In(_) => self.evaluate_scalar_constraint(
                    index, focus, &values, data, constraint, results, evaluation,
                )?,
                Constraint::Pair { .. }
                | Constraint::Node(_)
                | Constraint::Logical { .. }
                | Constraint::Qualified { .. }
                | Constraint::Closed(_) => self.evaluate_relational_constraint(
                    index, focus, &values, data, constraint, results, evaluation,
                )?,
            }
        }
        for &property in &shape.properties {
            if !values.is_empty()
                && evaluation.skip_recursive(
                    index,
                    "PropertyConstraintComponent",
                    property,
                    &self.shapes,
                )
            {
                continue;
            }
            for value in &values {
                self.evaluate_child(
                    index,
                    "PropertyConstraintComponent",
                    property,
                    value,
                    data,
                    results,
                    evaluation,
                )?;
            }
        }
        Ok(())
    }
}
