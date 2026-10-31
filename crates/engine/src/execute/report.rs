use super::*;

impl Violation {
    fn data_term(
        &self,
        value: &Term,
        display: &crate::display::Display<'_, impl GraphRead + ?Sized, impl GraphRead + ?Sized>,
    ) -> Result<String> {
        match self.data_description {
            DataDescription::Identity => Ok(value.to_string()),
            DataDescription::Expanded => display.term(value, false),
        }
    }
}

impl CompiledShapes {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn write_result(
        &self,
        report: &mut Dataset,
        parent: &Term,
        edge: &str,
        mut violation: Violation,
        data: &(impl GraphRead + ?Sized),
        text: &mut String,
        clones: &mut ReportClones,
        display: &crate::display::Display<'_, impl GraphRead + ?Sized, impl GraphRead + ?Sized>,
    ) -> Result<()> {
        let shape = &self.shapes[violation.shape];
        let result: Term = BlankNode::default().into();
        add(report, parent, iri(SH, edge), result.clone())?;
        for (predicate, object) in [
            (iri(RDF, "type"), iri(SH, "ValidationResult").into()),
            (
                iri(SH, "sourceConstraintComponent"),
                violation.component.clone().into(),
            ),
            (iri(SH, "sourceShape"), shape.node.clone()),
            (iri(SH, "resultSeverity"), shape.severity.clone().into()),
            (iri(SH, "focusNode"), violation.focus.clone()),
        ] {
            add(report, &result, predicate, object)?;
        }
        clones.copy(&self.graph, report, &shape.node)?;
        if matches!(violation.data_description, DataDescription::Expanded) {
            clones.copy(data, report, &violation.focus)?;
        }
        if let Some(path) = &violation.path {
            add(report, &result, iri(SH, "resultPath"), path.clone())?;
            clones.copy(&self.graph, report, path)?;
        }
        if let Some(value) = &violation.value {
            add(report, &result, iri(SH, "value"), value.clone())?;
            if matches!(violation.data_description, DataDescription::Expanded) {
                clones.copy(data, report, value)?;
            }
        }
        if let Some(constraint) = &violation.source_constraint {
            add(
                report,
                &result,
                iri(SH, "sourceConstraint"),
                constraint.clone(),
            )?;
            clones.copy(&self.graph, report, constraint)?;
        }
        let messages = if let Some(messages) = violation.messages.take() {
            messages
        } else if shape.messages.is_empty() {
            let message = if !violation.message.is_empty() {
                std::mem::take(&mut violation.message)
            } else if let Some(comparison) = &violation.comparison {
                match comparison {
                    ComparisonMessage::Range { operator, bound } => {
                        let operator = match operator {
                            crate::ComparisonOperator::Less => "<",
                            crate::ComparisonOperator::LessEqual => "<=",
                            crate::ComparisonOperator::Greater => ">",
                            crate::ComparisonOperator::GreaterEqual => ">=",
                        };
                        format!("Value is not {operator} {}", display.term(bound, true)?)
                    }
                    ComparisonMessage::PropertyPair {
                        operator,
                        predicate,
                    } => {
                        let operator = match operator {
                            crate::ComparisonOperator::Less => "<=",
                            crate::ComparisonOperator::LessEqual => "<",
                            _ => unreachable!("Property-pair ordering is less or less-equal"),
                        };
                        let value = violation
                            .value
                            .as_ref()
                            .expect("Ordering violation has value");
                        format!(
                            "Value of {}->{} {operator} {}",
                            display.term(&violation.focus, false)?,
                            display.iri(predicate),
                            display.term(value, false)?,
                        )
                    }
                }
            } else {
                let focus = violation.data_term(&violation.focus, display)?;
                let component = violation
                    .component
                    .as_str()
                    .strip_prefix(SH)
                    .unwrap_or(violation.component.as_str());
                match component {
                    "MinCountConstraintComponent" | "MaxCountConstraintComponent" => {
                        let minimum = component == "MinCountConstraintComponent";
                        let count = shape
                            .constraints
                            .iter()
                            .find_map(|constraint| match constraint {
                                Constraint::Count {
                                    minimum: is_minimum,
                                    count,
                                } if *is_minimum == minimum => Some(count),
                                _ => None,
                            })
                            .expect("cardinality violation requires a compiled count constraint");
                        let direction = if minimum { "Less" } else { "More" };
                        let location = if let Some(path) = violation.printed_path() {
                            format!("{focus}->{}", display.term(path, true)?)
                        } else {
                            focus
                        };
                        format!("{direction} than {count} values on {location}")
                    }
                    "MinLengthConstraintComponent" | "MaxLengthConstraintComponent" => {
                        let minimum = component == "MinLengthConstraintComponent";
                        let count = shape
                            .constraints
                            .iter()
                            .find_map(|constraint| match constraint {
                                Constraint::Length {
                                    minimum: is_minimum,
                                    count,
                                } if *is_minimum == minimum => Some(count),
                                _ => None,
                            })
                            .expect("length violation requires a compiled length constraint");
                        let number = count.number.to_string();
                        let lexical = count.literal.value();
                        let value = if lexical == number {
                            format!("\"{lexical}\"")
                        } else {
                            format!("\"{lexical}\" = {number}")
                        };
                        let operator = if minimum { ">=" } else { "<=" };
                        format!(
                            "String length not {operator} Literal({value}, datatype={})",
                            display.iri(&count.literal.datatype().into_owned())
                        )
                    }
                    "QualifiedMinCountConstraintComponent"
                    | "QualifiedMaxCountConstraintComponent" => {
                        let (target, minimum, maximum) = shape
                            .constraints
                            .iter()
                            .find_map(|constraint| match constraint {
                                Constraint::Qualified {
                                    shape,
                                    minimum,
                                    maximum,
                                    ..
                                } => Some((*shape, minimum, maximum)),
                                _ => None,
                            })
                            .expect("qualified violation requires a compiled qualified constraint");
                        let mut counts = String::new();
                        if let Some(minimum) = minimum {
                            counts.push_str(&format!(" MinCount {minimum}"));
                        }
                        if let Some(maximum) = maximum {
                            counts.push_str(&format!(" MaxCount {maximum}"));
                        }
                        format!(
                            "Focus node does not conform to shape{counts}: {}",
                            display.term(&self.shapes[target].node, true)?
                        )
                    }
                    "PatternConstraintComponent" => {
                        let patterns = values(&self.graph, &shape.node, "pattern")?
                            .iter()
                            .filter_map(crate::value::lexical)
                            .map(str::to_owned)
                            .collect::<Vec<_>>();
                        if patterns.len() == 1 {
                            format!("Value does not match pattern '{}'", patterns[0])
                        } else {
                            format!(
                                "Value does not match every pattern in ('{}')",
                                patterns.join("', '")
                            )
                        }
                    }
                    "UniqueLangConstraintComponent" => {
                        "More than one String shares the same Language".to_owned()
                    }
                    "ClassConstraintComponent" => {
                        let mut classes = values(&self.graph, &shape.node, "class")?
                            .iter()
                            .map(|n| display.term(n, true))
                            .collect::<Result<Vec<_>>>()?;
                        classes.sort();
                        if classes.len() == 1 {
                            format!("Value does not have class {}", classes[0])
                        } else {
                            format!("Value class is not in classes ({})", classes.join(", "))
                        }
                    }
                    "NodeKindConstraintComponent" => {
                        let kind = shape
                            .constraints
                            .iter()
                            .find_map(|constraint| {
                                if let Constraint::NodeKind(kind) = constraint {
                                    Some(kind)
                                } else {
                                    None
                                }
                            })
                            .expect("node kind violation requires a compiled node kind constraint");
                        format!("Value is not of Node Kind {}", display.iri(&iri(SH, kind)))
                    }
                    "DatatypeConstraintComponent" => {
                        let datatype = shape
                            .constraints
                            .iter()
                            .find_map(|constraint| {
                                if let Constraint::Datatype(datatype) = constraint {
                                    Some(datatype)
                                } else {
                                    None
                                }
                            })
                            .expect("datatype violation requires a compiled datatype constraint");
                        format!(
                            "Value is not Literal with datatype {}",
                            display.iri(datatype)
                        )
                    }
                    "InConstraintComponent" if shape.constraints.iter().any(|constraint| {
                        matches!(constraint, Constraint::In(allowed) if allowed.is_empty())
                    }) => {
                        let value = violation.value.as_ref().expect("membership violation has a value");
                        format!("Value {} not in list []", display.term(value, false)?)
                    }
                    "NodeConstraintComponent" => {
                        let mut nodes = values(&self.graph, &shape.node, "node")?
                            .iter()
                            .map(|n| display.term(n, true))
                            .collect::<Result<Vec<_>>>()?;
                        nodes.sort();
                        if nodes.len() == 1 {
                            format!(
                                "Value does not conform to Shape {}. See details for more information.",
                                nodes[0]
                            )
                        } else {
                            format!(
                                "Value must conform to every Shape in ('{}'). See details for more information.",
                                nodes.join("', '")
                            )
                        }
                    }
                    "NotConstraintComponent" => {
                        let mut nodes = values(&self.graph, &shape.node, "not")?
                            .iter()
                            .map(|n| display.term(n, true))
                            .collect::<Result<Vec<_>>>()?;
                        nodes.sort();
                        format!(
                            "Node {focus} must not conform to shape {}",
                            nodes.join(", ")
                        )
                    }
                    _ => format!("{component} violated at {focus}"),
                }
            };
            vec![Literal::new_simple_literal(message).into()]
        } else {
            shape.messages.clone()
        };
        let component = violation
            .component
            .as_str()
            .strip_prefix(SH)
            .unwrap_or(violation.component.as_str());
        let title = if [
            "QualifiedMinCountConstraintComponent",
            "QualifiedMaxCountConstraintComponent",
        ]
        .contains(&component)
        {
            "QualifiedValueShapeConstraintComponent"
        } else {
            component
        };
        let category = if shape.severity == iri(SH, "Violation") {
            "Constraint Violation"
        } else {
            "Validation Result"
        };
        text.push_str(&format!(
            "{category} in {title} ({}):\n\tSeverity: {}\n\tSource Shape: {}\n\tFocus Node: {}\n",
            violation.component.as_str(),
            display.iri(&shape.severity),
            display.term(&shape.node, true)?,
            violation.data_term(&violation.focus, display)?
        ));
        if let Some(value) = &violation.value {
            text.push_str(&format!(
                "\tValue Node: {}\n",
                violation.data_term(value, display)?
            ));
        }
        if let Some(path) = violation.printed_path() {
            text.push_str(&format!("\tResult Path: {}\n", display.term(path, true)?));
        }
        if let Some(source) = &violation.source_constraint {
            text.push_str(&format!(
                "\tSource Constraint: {}\n",
                display.term(source, true)?
            ));
        }
        let mut messages = messages;
        messages.sort_by_key(ToString::to_string);
        for message in messages {
            let printed = display.message(&message);
            text.push_str(&format!("\tMessage: {printed}\n"));
            add(report, &result, iri(SH, "resultMessage"), message)?;
        }
        let mut descriptions = Vec::new();
        for detail in violation.details {
            let mut description = String::new();
            self.write_result(
                report,
                &result,
                "detail",
                detail,
                data,
                &mut description,
                clones,
                display,
            )?;
            descriptions.push(description);
        }
        descriptions.sort();
        if !descriptions.is_empty() {
            text.push_str("\tDetails:\n");
        }
        for description in descriptions {
            for line in description.lines() {
                text.push_str(&format!("\t\t{line}\n"));
            }
        }
        Ok(())
    }
}
