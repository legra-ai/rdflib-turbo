use super::*;

impl CompiledShapes {
    /// Expand rules, then discover validation targets on the resulting graph.
    pub fn validate(&self, data: &mut Dataset, options: Options) -> Result<ValidationReport> {
        let warnings = self.expand_rules(data, options.clone())?;
        self.validate_expanded(data, options, warnings)
    }

    /// Validate a borrowed indexed source without copying it or permitting rule mutation.
    /// Named graphs participate through a streaming union; callbacks borrow the same source.
    pub fn validate_readonly(
        &self,
        data: &(impl GraphRead + ?Sized),
        options: Options,
    ) -> Result<ValidationReport> {
        if options.max_rule_iterations == 0 {
            return Err(Error::Invalid(
                "max_rule_iterations must be positive".into(),
            ));
        }
        if self.has_active_rules(&options) {
            return Err(Error::Invalid(
                "Readonly validation cannot execute active rules".into(),
            ));
        }
        self.validate_expanded(data, options, Vec::new())
    }

    pub(super) fn validate_expanded(
        &self,
        data: &(impl GraphRead + ?Sized),
        options: Options,
        warnings: Vec<String>,
    ) -> Result<ValidationReport> {
        let mut evaluation = Evaluation::new(options.max_validation_depth);
        evaluation.warnings = warnings;
        evaluation.comparison = options.comparison.clone();
        evaluation.abort = options
            .abort_on_first
            .then_some((options.allow_infos, options.allow_warnings));
        let display_data = data;
        let union = UnionGraph::new(data);
        let data = &union;
        evaluation.functions = self.functions.clone();
        let mut violations = Vec::new();
        for &index in &self.execution_order {
            let shape = &self.shapes[index];
            if !options.use_shapes.is_empty() && !options.use_shapes.contains(&shape.node) {
                continue;
            }
            if shape.deactivated {
                continue;
            }
            let targets = crate::SelectedTargetsRef::new(&shape.targets);
            let source_focus = if options.use_shapes.is_empty() || options.focus_nodes.is_empty() {
                if targets.is_core()
                    && (targets.classes().next().is_some()
                        || targets.implicit_classes().next().is_some())
                {
                    data.selected_focus_source()
                        .map(|source| {
                            source.acquire_focus(
                                targets,
                                (!options.focus_nodes.is_empty())
                                    .then_some(options.focus_nodes.as_slice()),
                            )
                        })
                        .transpose()?
                } else {
                    None
                }
            } else {
                None
            };
            let lexical_focus;
            let focus_nodes = if let Some(batch) = &source_focus {
                batch.words()
            } else {
                lexical_focus =
                    self.selected_focus_nodes(shape, data, &options, &mut evaluation)?;
                &lexical_focus
            };
            self.evaluate_batch(
                index,
                focus_nodes,
                data,
                &mut violations,
                &mut evaluation,
                None,
                source_focus.as_ref(),
            )?;
            drop(source_focus);
            if self.should_abort(&violations, &evaluation) {
                break;
            }
        }
        let conforms = violations.iter().all(|v| {
            let severity = &self.shapes[v.shape].severity;
            (options.allow_warnings && severity == &iri(SH, "Warning"))
                || ((options.allow_infos || options.allow_warnings) && severity == &iri(SH, "Info"))
        });
        let recursion_warnings = std::mem::take(&mut evaluation.warnings);
        drop(evaluation);
        let mut report = Dataset::new();
        let root: Term = BlankNode::default().into();
        add(
            &mut report,
            &root,
            iri(RDF, "type"),
            iri(SH, "ValidationReport").into(),
        )?;
        add(
            &mut report,
            &root,
            iri(SH, "conforms"),
            Literal::from(conforms).into(),
        )?;
        let mut text = format!(
            "Validation Report\nConforms: {}\n",
            if conforms { "True" } else { "False" }
        );
        if !violations.is_empty() {
            text.push_str(&format!("Results ({}):\n", violations.len()));
        }
        let mut shape_namespaces = self.namespaces.clone();
        let mut data_namespaces = options.namespaces;
        for (p, ns) in [("sh", SH), ("rdf", RDF), ("rdfs", RDFS), ("xsd", XSD)] {
            shape_namespaces.push((p.into(), ns.into()));
            data_namespaces.push((p.into(), ns.into()));
        }
        let display = crate::display::Display {
            shape_namespaces,
            data_namespaces,
            data: display_data,
            shapes: &self.graph,
        };
        let mut clones = ReportClones::default();
        let mut descriptions = Vec::new();
        let mut boolean_values = Vec::new();
        for violation in violations {
            let mut description = String::new();
            self.write_result(
                &mut report,
                &root,
                "result",
                violation,
                data,
                &mut description,
                &mut clones,
                &mut boolean_values,
                &display,
            )?;
            descriptions.push(description);
        }
        descriptions.sort();
        for description in descriptions {
            text.push_str(&description);
        }
        Ok(ValidationReport {
            conforms,
            graph: report,
            text,
            recursion_warnings,
            boolean_values,
        })
    }
}
