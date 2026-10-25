//! Rule-only execution on the existing transaction owner, with deferred acceptance.

use oxigraph::model::GraphName;
use oxigraph::store::Transaction;
use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyshacl_turbo_engine::{CompiledShapes, Options, ScopedGraph};
use std::sync::Arc;

use super::NativeTransaction;
use super::operations::{Operation, Reply};
use super::session::Session;
use super::shacl::ValidationDelta;
use crate::Result;

pub(super) fn expand(
    transaction: &Transaction<'_>,
    shapes: &CompiledShapes,
    options: Options,
    graph: Option<GraphName>,
) -> Result<Reply> {
    let output = if let Some(graph) = &graph {
        shapes.expand_staged(&ScopedGraph::new(transaction, graph.clone()), options)?
    } else {
        shapes.expand_staged(transaction, options)?
    };
    Ok(Reply::StagedRules(Box::new((
        output.recursion_warnings,
        ValidationDelta {
            data: output.graph,
            inference: Default::default(),
            graph,
        },
    ))))
}

impl NativeTransaction {
    pub(crate) fn prepare_rules(
        &self,
        shapes: Arc<CompiledShapes>,
        options: Options,
        graph: Option<GraphName>,
    ) -> Result<PreparedRules> {
        match self.session.execute(Operation::ExpandRules {
            shapes,
            options,
            graph,
        })? {
            Reply::PreparedRules(output) => {
                let (warnings, revision) = *output;
                Ok(PreparedRules {
                    session: self.session.clone(),
                    warnings: Some(warnings),
                    revision: Some(revision),
                })
            }
            _ => unreachable!("native prepared rule output type"),
        }
    }
}

/// Retain the caller's staged rule output until Python accepts its warnings.
#[pyclass(module = "rdflib_turbo._native")]
pub(crate) struct PreparedRules {
    session: Arc<Session>,
    warnings: Option<Vec<String>>,
    revision: Option<u64>,
}

#[pymethods]
impl PreparedRules {
    fn output(&mut self) -> PyResult<Vec<String>> {
        self.warnings
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Rule output was already consumed"))
    }

    fn accept(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.warnings.is_some() {
            return Err(PyRuntimeError::new_err(
                "Rule output must be consumed before acceptance",
            ));
        }
        let revision = self
            .revision
            .take()
            .ok_or_else(|| PyRuntimeError::new_err("Rule output was already accepted"))?;
        py.detach(|| self.session.accept_validation(revision))
            .map_err(crate::error)
    }
}

impl Drop for PreparedRules {
    fn drop(&mut self) {
        if self.revision.is_some() {
            let _ = self.session.finish(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use oxigraph::io::RdfFormat;
    use oxigraph::model::*;
    use oxigraph::store::Store;
    use pyshacl_turbo_engine::{CompiledShapes, IndexedShapesSource, Options};

    use crate::store::operations::{Operation, Reply};
    use crate::store::session::{Control, Session};

    fn node(value: &str) -> NamedNode {
        NamedNode::new(format!("urn:{value}")).unwrap()
    }

    fn shapes() -> Arc<CompiledShapes> {
        let source = IndexedShapesSource::parse_graph(
            "@prefix sh:<http://www.w3.org/ns/shacl#>. <urn:S> a sh:NodeShape; sh:targetSubjectsOf <urn:p>; sh:class <urn:Missing>; sh:rule [a sh:TripleRule; sh:subject sh:this; sh:predicate <urn:q>; sh:object [sh:path <urn:p>]].",
            RdfFormat::Turtle,
            None,
        ).unwrap();
        Arc::new(CompiledShapes::compile(source, true).unwrap())
    }

    #[test]
    fn scoped_rules_stage_only_derived_facts_without_running_constraints() {
        for graph in [
            GraphName::DefaultGraph,
            node("selected").into(),
            BlankNode::new("selected").unwrap().into(),
        ] {
            let store = Store::new().unwrap();
            let mut transaction = store.start_transaction().unwrap();
            let value = Literal::new_typed_literal("01", node("datatype"));
            transaction.insert(&Quad::new(
                node("s"),
                node("p"),
                value.clone(),
                graph.clone(),
            ));
            transaction.insert(&Quad::new(
                node("other"),
                node("p"),
                node("outside"),
                node("hidden"),
            ));
            let Reply::StagedRules(output) = (Operation::ExpandRules {
                shapes: shapes(),
                options: Options::default(),
                graph: Some(graph.clone()),
            })
            .execute(&mut transaction)
            .unwrap() else {
                panic!("rule-only operation returned a validation report");
            };
            let (warnings, additions) = *output;
            assert!(warnings.is_empty());
            let derived = Quad::new(node("s"), node("q"), value, graph);
            assert!(!transaction.contains(&derived).unwrap());
            assert_eq!(additions.data.len(), 1);
            additions.apply(&mut transaction);
            assert!(transaction.contains(&derived).unwrap());
            assert_eq!(transaction.len().unwrap(), 3);
        }
    }

    #[test]
    fn owner_keeps_rule_output_private_until_revision_checked_acceptance() {
        let store = Store::new().unwrap();
        store
            .insert(&Quad::new(
                node("s"),
                node("p"),
                node("value"),
                GraphName::DefaultGraph,
            ))
            .unwrap();
        let owner = Session::start(Arc::new(Mutex::new(Control::new(store.clone())))).unwrap();
        let Reply::PreparedRules(output) = owner
            .execute(Operation::ExpandRules {
                shapes: shapes(),
                options: Options::default(),
                graph: None,
            })
            .unwrap()
        else {
            panic!("missing prepared rule output");
        };
        let (warnings, revision) = *output;
        assert!(warnings.is_empty());
        let derived = Quad::new(node("s"), node("q"), node("value"), GraphName::DefaultGraph);
        assert!(!store.contains(&derived).unwrap());
        owner.accept_validation(revision).unwrap();
        assert!(!store.contains(&derived).unwrap());
        owner.finish(true).unwrap();
        assert!(store.contains(&derived).unwrap());
    }
}
