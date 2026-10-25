use super::*;

impl CompiledShapes {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn evaluate_scalar_constraint(
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
            Constraint::Count { minimum, count } => {
                if !count.allows(values.len(), *minimum) {
                    results.push(violation(
                        if *minimum {
                            "MinCountConstraintComponent"
                        } else {
                            "MaxCountConstraintComponent"
                        },
                        None,
                    ));
                }
            }
            Constraint::HasValue(required) => {
                if !values.contains(required) {
                    results.push(violation("HasValueConstraintComponent", None));
                }
            }
            Constraint::Class(class) => {
                for value in values {
                    if !evaluation.is_instance(data, value, class)? {
                        results.push(violation("ClassConstraintComponent", Some(value)));
                    }
                }
            }
            Constraint::Datatype(datatype) => {
                for value in values {
                    if !crate::value::datatype(value, datatype) {
                        results.push(violation("DatatypeConstraintComponent", Some(value)));
                    }
                }
            }
            Constraint::NodeKind(kind) => {
                for value in values {
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
                        results.push(violation("NodeKindConstraintComponent", Some(value)));
                    }
                }
            }
            Constraint::SingleLine => {
                for value in values {
                    if let Term::Literal(literal) = value
                        && literal
                            .value()
                            .bytes()
                            .any(|byte| (b'\n'..=b'\r').contains(&byte))
                    {
                        results.push(violation("SingleLineConstraintComponent", Some(value)));
                    }
                }
            }
            Constraint::Pattern(regex) => {
                for value in values {
                    let valid = crate::value::lexical(value)
                        .map(|text| regex.is_match(text))
                        .transpose()?
                        .unwrap_or(false);
                    if !valid {
                        results.push(violation("PatternConstraintComponent", Some(value)));
                    }
                }
            }
            Constraint::Range {
                bound,
                operator,
                component,
                query,
            } => {
                for value in values {
                    if !crate::value::compare(
                        &evaluation.comparison,
                        crate::ComparisonRole::Range,
                        *operator,
                        query,
                        data,
                        value,
                        bound,
                    )? {
                        results.push(violation(component, Some(value)).with_comparison(
                            ComparisonMessage::Range {
                                operator: *operator,
                                bound: bound.clone(),
                            },
                        ));
                    }
                }
            }
            Constraint::Length { minimum, count } => {
                for value in values {
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
                            Some(value),
                        ));
                    }
                }
            }
            Constraint::LanguageIn(ranges) => {
                for value in values {
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
                        results.push(violation("LanguageInConstraintComponent", Some(value)));
                    }
                }
            }
            Constraint::UniqueLang => {
                let mut seen = HashSet::new();
                let mut repeated = HashSet::new();
                for value in values {
                    if let Term::Literal(l) = value
                        && let Some(lang) = l.language()
                        && !seen.insert(lang)
                    {
                        repeated.insert(lang);
                    }
                }
                for _ in repeated {
                    results.push(violation("UniqueLangConstraintComponent", None));
                }
            }
            Constraint::In(allowed) => {
                for value in values {
                    if !allowed.contains(value) {
                        results.push(violation("InConstraintComponent", Some(value)));
                    }
                }
            }
            _ => unreachable!("constraint dispatched to its compiled category"),
        }
        Ok(())
    }
}
