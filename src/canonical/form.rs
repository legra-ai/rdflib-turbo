//! Canonical N-Quads built from the stored terms, never from normalized values.
use std::fmt::Display;

use oxigraph::model::vocab::xsd;
use oxigraph::model::{
    BlankNode, Dataset, GeneralizedQuad, GraphName, Literal, NamedOrBlankNode, Quad, QuadRef, Term,
};
use oxigraph::store::Store;

use super::request::{CanonicalRequest, CanonicalScope, TermIdentity};
use crate::{Error, Result};

/// The selected quads of one canonicalization, before blank-node labelling.
pub(crate) struct CanonicalForm {
    // bounded: RDFC-1.0 labels blank nodes from the whole input and sorts the
    // whole output, so the selected scope is necessarily held once.
    dataset: Dataset,
}

impl CanonicalForm {
    /// Collects the selected quads, rejecting generalized statements.
    pub(crate) fn collect<E: Display>(
        quads: impl IntoIterator<Item = std::result::Result<GeneralizedQuad, E>>,
        scope: &CanonicalScope,
        identity: TermIdentity,
    ) -> Result<Self> {
        let mut dataset = Dataset::new();
        for quad in quads {
            let quad = quad.map_err(|error| Error::Runtime(error.to_string()))?;
            let mut quad = Quad::try_from(quad).map_err(|_| {
                Error::Invalid(
                    "RDFC-1.0 canonicalization requires RDF statements; \
                     generalized subjects, predicates or graph names are unsupported"
                        .into(),
                )
            })?;
            if !scope.keeps_graph_names() {
                quad.graph_name = GraphName::DefaultGraph;
            }
            if identity == TermIdentity::Rdf11 {
                quad.object = rdf11_term(quad.object);
            }
            dataset.insert(&quad);
        }
        Ok(Self { dataset })
    }

    /// Labels blank nodes within the request's work budget and renders the
    /// sorted canonical N-Quads document.
    pub(crate) fn render(&self, request: CanonicalRequest) -> Result<String> {
        // bounded: one line per selected quad; the document itself is the output.
        let mut lines = self
            .relabeled(request)?
            .map(|quad| format!("{quad} .\n"))
            .collect::<Vec<_>>();
        lines.sort_unstable();
        Ok(lines.concat())
    }

    /// A new memory store holding the canonically relabelled quads.
    pub(crate) fn store(&self, request: CanonicalRequest) -> Result<Store> {
        let quads = self.relabeled(request)?;
        let store = Store::new().map_err(runtime)?;
        let mut output = store.start_transaction().map_err(runtime)?;
        for quad in quads {
            output.insert(&quad);
        }
        output.commit().map_err(runtime)?;
        Ok(store)
    }

    fn relabeled(&self, request: CanonicalRequest) -> Result<impl Iterator<Item = Quad> + '_> {
        let labels = self
            .dataset
            .canonicalize_blank_nodes_within(request.algorithm(), request.budget())
            .map_err(Error::CanonicalBudget)?;
        let relabel = move |node: BlankNode| -> BlankNode {
            labels
                .get(&node.as_ref())
                .cloned()
                .unwrap_or_else(|| unreachable!("RDFC-1.0 labels every blank node of its input"))
        };
        Ok(self
            .dataset
            .iter()
            .map(move |quad| relabel_quad(quad, &relabel)))
    }
}

fn runtime(error: impl Display) -> Error {
    Error::Runtime(error.to_string())
}

fn rdf11_term(term: Term) -> Term {
    match term {
        Term::Literal(literal)
            if literal.datatype() == xsd::STRING && literal.language().is_none() =>
        {
            Literal::new_simple_literal(literal.value()).into()
        }
        other => other,
    }
}

fn relabel_quad(quad: QuadRef<'_>, relabel: &impl Fn(BlankNode) -> BlankNode) -> Quad {
    let subject = match quad.subject.into_owned() {
        NamedOrBlankNode::BlankNode(node) => relabel(node).into(),
        other => other,
    };
    let object = match quad.object.into_owned() {
        Term::BlankNode(node) => relabel(node).into(),
        other => other,
    };
    let graph_name = match quad.graph_name.into_owned() {
        GraphName::BlankNode(node) => relabel(node).into(),
        other => other,
    };
    Quad::new(subject, quad.predicate.into_owned(), object, graph_name)
}
