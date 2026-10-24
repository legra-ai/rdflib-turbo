//! RDFLib node bool, using the shared default scalar conversion policy.
use oxigraph::model::Term;

pub(crate) fn truthy(term: &Term, integer_limit: usize) -> bool {
    match term {
        Term::NamedNode(node) => !node.as_str().is_empty(),
        Term::BlankNode(node) => !node.as_str().is_empty(),
        Term::Literal(literal) => super::truthy(literal, integer_limit),
    }
}
