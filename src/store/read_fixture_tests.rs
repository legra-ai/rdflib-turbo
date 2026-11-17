//! Store fixtures and observed-row projections shared by the read tests.
use oxigraph::model::{GraphName, Literal, NamedNode, Quad};
use oxigraph::store::{LexicalTerm, Store};

use super::super::TermView;

pub(super) fn open(
    store: &Store,
    subject: Option<TermView<'_>>,
    predicate: Option<TermView<'_>>,
    object: Option<TermView<'_>>,
    graph: Option<TermView<'_>>,
    mask: u8,
) -> crate::Result<super::super::ReadCursor> {
    let pattern = super::super::PreparedRead::new(subject, predicate, object, graph)?;
    super::super::open(store, &pattern, mask)
}

pub(super) fn uri(value: &str) -> TermView<'_> {
    TermView::new("uri", value, None, None)
}

pub(super) type ObservedTerm = (u8, String, Option<String>, Option<String>);
pub(super) type ObservedRow = (
    Option<ObservedTerm>,
    Option<ObservedTerm>,
    Option<ObservedTerm>,
);

pub(super) fn output_uri(value: &str) -> ObservedTerm {
    (0, value.into(), None, None)
}

pub(super) fn observed_term(term: &Option<LexicalTerm>) -> Option<ObservedTerm> {
    term.as_ref().map(|term| match term {
        LexicalTerm::NamedNode(value) => output_uri(value.as_str()),
        LexicalTerm::BlankNode(value) => (1, value.as_str().into(), None, None),
        LexicalTerm::Literal {
            value,
            datatype,
            language,
        } => (
            2,
            value.as_str().into(),
            datatype.as_ref().map(|text| text.as_str().into()),
            language.as_ref().map(|text| text.as_str().into()),
        ),
    })
}

pub(super) fn observed_rows(rows: &[super::super::Row]) -> Vec<ObservedRow> {
    // bounded: every tested native batch is capped at 128 rows.
    rows.iter()
        .map(|(s, p, o)| (observed_term(s), observed_term(p), observed_term(o)))
        .collect()
}

pub(super) fn observed_batch(
    (rows, finished): (Vec<super::super::Row>, bool),
) -> (Vec<ObservedRow>, bool) {
    (observed_rows(&rows), finished)
}

pub(super) fn insert(store: &Store, subject: &str, object: Literal, graph: GraphName) {
    store
        .insert(&Quad::new(
            NamedNode::new_unchecked(subject),
            NamedNode::new_unchecked("urn:p"),
            object,
            graph,
        ))
        .unwrap();
}
