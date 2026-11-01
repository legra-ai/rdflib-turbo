use super::*;

impl CompiledShapes {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate_scalar_constraint(
        &self,
        index: usize,
        focus: &Term,
        values: &crate::observed_values::ObservedValues,
        data: &(impl GraphRead + ?Sized),
        constraint: &Constraint,
        results: &mut Vec<Violation>,
        evaluation: &mut Evaluation,
        focus_observation: Option<&crate::BooleanObservation>,
    ) -> Result<()> {
        let shape = &self.shapes[index];
        let violation = |component,
                         value: Option<crate::observed_values::ValueSlot<'_>>|
         -> Result<Violation> {
            Violation::new(
                index,
                shape,
                focus,
                component,
                value.map(|slot| slot.term()),
            )
            .with_observations(focus_observation, value.and_then(|slot| slot.observation()))
        };
        match constraint {
            Constraint::Count { minimum, count } => {
                if !count.allows(values.len(), *minimum) {
                    results.push(violation(
                        if *minimum {
                            "MinCountConstraintComponent"
                        } else {
                            "MaxCountConstraintComponent"
                        },
                        None,
                    )?);
                }
            }
            Constraint::HasValue(required) => {
                if !values.contains(required) {
                    results.push(violation("HasValueConstraintComponent", None)?);
                }
            }
            Constraint::Class(class) => {
                for slot in values.slots() {
                    let value = slot.term();
                    if !evaluation.is_instance(data, value, class)? {
                        results.push(violation("ClassConstraintComponent", Some(slot))?);
                    }
                }
            }
            Constraint::Datatype(datatype) => {
                for slot in values.slots() {
                    if !slot.datatype(datatype)? {
                        results.push(violation("DatatypeConstraintComponent", Some(slot))?);
                    }
                }
            }
            Constraint::NodeKind(kind) => {
                for slot in values.slots() {
                    let value = slot.term();
                    let valid = match value {
                        Term::NamedNode(_) => {
                            ["IRI", "BlankNodeOrIRI", "IRIOrLiteral"].contains(&kind.as_str())
                        }
                        Term::BlankNode(_) => ["BlankNode", "BlankNodeOrIRI", "BlankNodeOrLiteral"]
                            .contains(&kind.as_str()),
                        Term::Literal(_) => ["Literal", "BlankNodeOrLiteral", "IRIOrLiteral"]
                            .contains(&kind.as_str()),
                    };
                    if !valid {
                        results.push(violation("NodeKindConstraintComponent", Some(slot))?);
                    }
                }
            }
            Constraint::SingleLine => {
                for slot in values.slots() {
                    let value = slot.term();
                    if let Term::Literal(literal) = value
                        && literal
                            .value()
                            .bytes()
                            .any(|byte| (b'\n'..=b'\r').contains(&byte))
                    {
                        results.push(violation("SingleLineConstraintComponent", Some(slot))?);
                    }
                }
            }
            Constraint::Pattern(regex) => {
                for slot in values.slots() {
                    let value = slot.term();
                    let valid = crate::value::lexical(value)
                        .map(|text| regex.is_match(text))
                        .transpose()?
                        .unwrap_or(false);
                    if !valid {
                        results.push(violation("PatternConstraintComponent", Some(slot))?);
                    }
                }
            }
            Constraint::Range {
                bound,
                operator,
                component,
                query,
            } => {
                for slot in values.slots() {
                    let value = slot.term();
                    if !crate::value::compare(
                        &evaluation.comparison,
                        crate::ComparisonRole::Range,
                        *operator,
                        query,
                        data,
                        value,
                        bound,
                    )? {
                        results.push(violation(component, Some(slot))?.with_comparison(
                            ComparisonMessage::Range {
                                operator: *operator,
                                bound: bound.clone(),
                            },
                        ));
                    }
                }
            }
            Constraint::Length { minimum, count } => {
                for slot in values.slots() {
                    let value = slot.term();
                    let valid = (*minimum && count.is_zero())
                        || crate::value::lexical(value)
                            .is_some_and(|s| count.allows(s.chars().count(), *minimum));
                    if !valid {
                        results.push(violation(
                            if *minimum {
                                "MinLengthConstraintComponent"
                            } else {
                                "MaxLengthConstraintComponent"
                            },
                            Some(slot),
                        )?);
                    }
                }
            }
            Constraint::LanguageIn(ranges) => {
                for slot in values.slots() {
                    let value = slot.term();
                    let valid = if let Term::Literal(l) = value {
                        l.language().is_some_and(|lang| {
                            ranges.iter().any(|r| {
                                r == "*"
                                    || lang == r
                                    || lang
                                        .strip_prefix(r)
                                        .is_some_and(|suffix| suffix.starts_with('-'))
                            })
                        })
                    } else {
                        false
                    };
                    if !valid {
                        results.push(violation("LanguageInConstraintComponent", Some(slot))?);
                    }
                }
            }
            Constraint::UniqueLang => {
                let mut seen = HashSet::new();
                let mut repeated = HashSet::new();
                for slot in values.slots() {
                    let value = slot.term();
                    if let Term::Literal(l) = value
                        && let Some(lang) = l.language()
                        && !seen.insert(lang)
                    {
                        repeated.insert(lang);
                    }
                }
                for _ in repeated {
                    results.push(violation("UniqueLangConstraintComponent", None)?);
                }
            }
            Constraint::In(allowed) => {
                for slot in values.slots() {
                    let value = slot.term();
                    if !allowed.contains(value) {
                        results.push(violation("InConstraintComponent", Some(slot))?);
                    }
                }
            }
            _ => unreachable!("constraint dispatched to its compiled category"),
        }
        Ok(())
    }
}
