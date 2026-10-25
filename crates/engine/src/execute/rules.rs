use super::*;

impl CompiledShapes {
    /// Mutates only this explicitly supplied store. Python validates a fresh store
    /// and commits additions to the caller's Graph only after successful execution.
    /// Whether validation needs a mutable working dataset for selected SHACL rules.
    pub fn has_active_rules(&self, options: &Options) -> bool {
        self.execution_order.iter().any(|&index| {
            let shape = &self.shapes[index];
            (options.use_shapes.is_empty() || options.use_shapes.contains(&shape.node))
                && shape.rules.iter().any(|rule| !rule.deactivated)
        })
    }

    pub fn expand_rules(&self, data: &mut Dataset, options: Options) -> Result<Vec<String>> {
        self.expand_rules_inner(data, options, None)
    }

    /// Expand rules while retaining only successfully inserted statements.
    pub fn expand_rules_delta(
        &self,
        data: &mut Dataset,
        options: Options,
    ) -> Result<RuleExpansion> {
        let mut graph = Dataset::new();
        let recursion_warnings = self.expand_rules_inner(data, options, Some(&mut graph))?;
        Ok(RuleExpansion {
            graph,
            recursion_warnings,
        })
    }

    pub(super) fn expand_rules_inner(
        &self,
        data: &mut (impl GraphWrite + ?Sized),
        options: Options,
        mut delta: Option<&mut Dataset>,
    ) -> Result<Vec<String>> {
        if options.max_rule_iterations == 0 {
            return Err(Error::Invalid(
                "max_rule_iterations must be positive".into(),
            ));
        }
        let mut evaluation = Evaluation::new(options.max_validation_depth);
        evaluation.comparison = options.comparison.clone();
        for &index in &self.execution_order {
            let shape = &self.shapes[index];
            if !options.use_shapes.is_empty() && !options.use_shapes.contains(&shape.node) {
                continue;
            }
            if shape.rules.is_empty() {
                continue;
            }
            let mut pass = 0;
            loop {
                if pass == options.max_rule_iterations {
                    return Err(Error::Runtime(format!(
                        "Rule iteration limit ({}) exceeded for {}",
                        options.max_rule_iterations, shape.node
                    )));
                }
                pass += 1;
                let mut changed = false;
                for rule in &shape.rules {
                    if rule.deactivated {
                        continue;
                    }
                    let view = UnionGraph::new(&*data);
                    let focus =
                        self.selected_focus_nodes(shape, &view, &options, &mut evaluation)?;
                    let functions = self.functions.clone();
                    evaluation.functions = functions.clone();
                    let mut additions = HashSet::new();
                    for node in focus {
                        let mut condition_results = Vec::new();
                        for &condition in &rule.conditions {
                            self.evaluate_batch(
                                condition,
                                std::slice::from_ref(&node),
                                &view,
                                &mut condition_results,
                                &mut evaluation,
                            )?;
                        }
                        if !condition_results.is_empty() {
                            continue;
                        }
                        if let Some(expressions) = &rule.triple {
                            let mut filter = |i, n: &Term| {
                                let mut results = Vec::new();
                                self.evaluate_batch(
                                    i,
                                    std::slice::from_ref(n),
                                    &view,
                                    &mut results,
                                    &mut evaluation,
                                )?;
                                Ok(results.is_empty())
                            };
                            let subjects =
                                expressions[0].evaluate(&node, &view, &functions, &mut filter)?;
                            let predicates =
                                expressions[1].evaluate(&node, &view, &functions, &mut filter)?;
                            let objects =
                                expressions[2].evaluate(&node, &view, &functions, &mut filter)?;
                            for s in subjects {
                                for p in &predicates {
                                    for o in &objects {
                                        additions.insert(Quad::new(
                                            subject(&s)?,
                                            crate::constraints::named(
                                                p.clone(),
                                                "TripleRule predicate",
                                            )?,
                                            o.clone(),
                                            GraphName::DefaultGraph,
                                        ));
                                    }
                                }
                            }
                        }
                        for query in &rule.queries {
                            let result = query
                                .prepare(&functions)
                                .on_queryable_dataset(&view)
                                .substitute_variable(Variable::new_unchecked("this"), node.clone())
                                .execute()?;
                            let QueryResults::Graph(triples) = result else {
                                return Err(Error::Invalid(
                                    "SPARQL rule did not return CONSTRUCT triples".into(),
                                ));
                            };
                            for triple in triples {
                                let triple = triple?;
                                additions.insert(Quad::new(
                                    triple.subject,
                                    triple.predicate,
                                    triple.object,
                                    GraphName::DefaultGraph,
                                ));
                            }
                        }
                    }
                    // All focus nodes of a rule see the same phase; subsequent
                    // rules see the complete batch of additions.
                    let mut rule_changed = false;
                    for quad in additions {
                        let present = data
                            .matching_quads(
                                Some(quad.subject.as_ref().into()),
                                Some(quad.predicate.as_ref().into()),
                                Some(quad.object.as_ref()),
                                None,
                            )
                            .next()
                            .transpose()?
                            .is_some();
                        if !present && data.insert_quad(quad.as_ref())? {
                            rule_changed = true;
                            if let Some(delta) = delta.as_deref_mut() {
                                delta.insert(&quad);
                            }
                            changed = true;
                        }
                    }
                    if rule_changed {
                        evaluation.clear_traversals();
                    }
                }
                if !options.iterate_rules || !changed {
                    break;
                }
            }
        }
        Ok(evaluation.warnings)
    }
}
