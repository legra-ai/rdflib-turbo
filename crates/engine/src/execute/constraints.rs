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
        observation: Option<crate::BooleanObservation>,
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
            observation.as_ref(),
            None,
        );
        evaluation.path.pop();
        result
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate_batch(
        &self,
        index: usize,
        focus: &[Term],
        data: &(impl GraphRead + ?Sized),
        results: &mut Vec<Violation>,
        evaluation: &mut Evaluation,
        inherited: Option<&crate::BooleanObservation>,
        source_focus: Option<&crate::SelectedFocusBatch>,
    ) -> Result<()> {
        let shape = &self.shapes[index];
        if shape.deactivated || focus.is_empty() {
            return Ok(());
        }
        let selected_sequence = shape
            .path
            .as_ref()
            .and_then(crate::SelectedPathRef::predicate_sequence);
        let selected_frame =
            if matches!(shape.path, Some(Path::Predicate(_))) || selected_sequence.is_some() {
                if let Some(batch) = source_focus {
                    Some(crate::selected_values::SelectedFrameGuard::Borrowed(
                        batch.values(),
                    ))
                } else {
                    if let Some(source) = data.selected_value_source() {
                        Some(crate::selected_values::SelectedFrameGuard::Owned(
                            source.begin_frame()?,
                        ))
                    } else {
                        inherited
                            .map(crate::BooleanObservation::begin_nested)
                            .transpose()?
                            .flatten()
                            .map(crate::selected_values::SelectedFrameGuard::Owned)
                    }
                }
            } else {
                None
            };
        // Match the reference batch boundary only for an active abort policy.
        // Values belong to this invocation, never a validation-wide predicate cache.
        let prepared =
            if evaluation.abort.is_some() || selected_frame.is_some() || source_focus.is_some() {
                if evaluation.path.len() >= evaluation.max_depth {
                    return Err(Error::Runtime(format!(
                        "Validation path too deep!\n{}",
                        evaluation.describe_path(&self.shapes, None)
                    )));
                }
                Some(
                    focus
                        .iter()
                        .enumerate()
                        .map(|(focus_index, node)| {
                            let observation = source_focus
                                .map(|batch| {
                                    batch
                                        .slot(focus_index)
                                        .map(crate::BooleanObservation::Focus)
                                })
                                .transpose()?;
                            let inherited = observation.as_ref().or(inherited);
                            match (&shape.path, &selected_frame) {
                                (Some(Path::Predicate(predicate)), Some(frame)) => {
                                    crate::observed_values::ObservedValues::selected(
                                        frame.frame(),
                                        node,
                                        predicate,
                                        inherited,
                                    )
                                }
                                (Some(_), Some(frame)) if selected_sequence.is_some() => {
                                    crate::observed_values::ObservedValues::selected_sequence(
                                        frame.frame(),
                                        node,
                                        selected_sequence.expect("checked predicate sequence"),
                                        inherited,
                                    )
                                }
                                (Some(path), _) => evaluation.values(path, data, node),
                                (None, _) => Ok(crate::observed_values::ObservedValues::focus(
                                    node.clone(),
                                    inherited.cloned(),
                                )),
                            }
                        })
                        .collect::<Result<Vec<_>>>()?,
                )
            } else {
                None
            };
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
            if let Some(values) = &prepared {
                has_values = values.iter().any(|values| !values.is_empty());
            } else if let Some(path) = &shape.path {
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
        let result = if let Some(values) = prepared {
            self.evaluate_component_batch(
                index,
                focus,
                &values,
                data,
                results,
                evaluation,
                inherited,
                source_focus,
            )
        } else {
            focus.iter().try_for_each(|node| {
                self.evaluate_inner(index, node, data, results, evaluation, inherited)
            })
        };
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
        inherited: Option<&crate::BooleanObservation>,
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
            crate::observed_values::ObservedValues::focus(focus.clone(), inherited.cloned())
        };
        for constraint in &shape.constraints {
            self.evaluate_constraint(
                index, focus, &values, data, constraint, results, evaluation, inherited,
            )?;
        }
        self.evaluate_properties(index, &values, data, results, evaluation)
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_constraint(
        &self,
        index: usize,
        focus: &Term,
        values: &crate::observed_values::ObservedValues,
        data: &(impl GraphRead + ?Sized),
        constraint: &Constraint,
        results: &mut Vec<Violation>,
        evaluation: &mut Evaluation,
        inherited: Option<&crate::BooleanObservation>,
    ) -> Result<()> {
        match constraint {
            Constraint::MemberShape(_)
            | Constraint::ListLength { .. }
            | Constraint::UniqueMembers(_) => self.evaluate_list_constraint(
                index, focus, values, data, constraint, results, evaluation,
            )?,
            Constraint::NodeExpression { .. } | Constraint::Sparql(_) => self
                .evaluate_advanced_constraint(
                    index, focus, values, data, constraint, results, evaluation,
                )?,
            Constraint::Count { .. }
            | Constraint::HasValue(_)
            | Constraint::Class(_)
            | Constraint::Datatype(_)
            | Constraint::NodeKind(_)
            | Constraint::Pattern(_)
            | Constraint::Range { .. }
            | Constraint::Length { .. }
            | Constraint::LanguageIn(_)
            | Constraint::UniqueLang
            | Constraint::SingleLine
            | Constraint::In(_) => self.evaluate_scalar_constraint(
                index, focus, values, data, constraint, results, evaluation, inherited,
            )?,
            Constraint::Pair { .. }
            | Constraint::Node(_)
            | Constraint::Logical { .. }
            | Constraint::Qualified { .. }
            | Constraint::Closed(_) => self.evaluate_relational_constraint(
                index, focus, values, data, constraint, results, evaluation, inherited,
            )?,
        }
        Ok(())
    }

    fn evaluate_properties(
        &self,
        index: usize,
        values: &crate::observed_values::ObservedValues,
        data: &(impl GraphRead + ?Sized),
        results: &mut Vec<Violation>,
        evaluation: &mut Evaluation,
    ) -> Result<()> {
        let shape = &self.shapes[index];
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
            for slot in values.slots() {
                let value = slot.term();
                self.evaluate_child(
                    index,
                    "PropertyConstraintComponent",
                    property,
                    value,
                    data,
                    results,
                    evaluation,
                    slot.observation().cloned(),
                )?;
            }
        }
        Ok(())
    }

    pub(super) fn should_abort(&self, results: &[Violation], evaluation: &Evaluation) -> bool {
        let Some((infos, warnings)) = evaluation.abort else {
            return false;
        };
        results.iter().any(|result| {
            let severity = self.shapes[result.shape].severity.as_str().strip_prefix(SH);
            !((severity == Some("Warning") && warnings)
                || (severity == Some("Info") && (infos || warnings)))
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn evaluate_component_batch(
        &self,
        index: usize,
        focus: &[Term],
        values: &[crate::observed_values::ObservedValues],
        data: &(impl GraphRead + ?Sized),
        results: &mut Vec<Violation>,
        evaluation: &mut Evaluation,
        inherited: Option<&crate::BooleanObservation>,
        source_focus: Option<&crate::SelectedFocusBatch>,
    ) -> Result<()> {
        let constraints = &self.shapes[index].constraints;
        let mut begin = 0;
        while begin < constraints.len() {
            let mut end = begin + 1;
            while end < constraints.len() && constraints[begin].same_component(&constraints[end]) {
                end += 1;
            }
            let previous = results.len();
            for constraint in &constraints[begin..end] {
                for (focus_index, (node, values)) in focus.iter().zip(values).enumerate() {
                    let observation = source_focus
                        .map(|batch| {
                            batch
                                .slot(focus_index)
                                .map(crate::BooleanObservation::Focus)
                        })
                        .transpose()?;
                    let inherited = observation.as_ref().or(inherited);
                    self.evaluate_constraint(
                        index, node, values, data, constraint, results, evaluation, inherited,
                    )?;
                }
            }
            if self.should_abort(&results[previous..], evaluation) {
                return Ok(());
            }
            begin = end;
        }
        // sh:property is one component too: finish all its nested shapes/focuses.
        for values in values {
            self.evaluate_properties(index, values, data, results, evaluation)?;
        }
        Ok(())
    }
}

impl Constraint {
    /// Compiled adjacent declarations belonging to one reference component.
    fn same_component(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Count { minimum: a, .. }, Self::Count { minimum: b, .. })
            | (Self::Length { minimum: a, .. }, Self::Length { minimum: b, .. })
            | (Self::ListLength { minimum: a, .. }, Self::ListLength { minimum: b, .. }) => a == b,
            (Self::Range { component: a, .. }, Self::Range { component: b, .. }) => a == b,
            (Self::Pair { kind: a, .. }, Self::Pair { kind: b, .. })
            | (Self::Logical { kind: a, .. }, Self::Logical { kind: b, .. }) => a == b,
            (Self::Sparql(a), Self::Sparql(b)) => a.component == b.component,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}
