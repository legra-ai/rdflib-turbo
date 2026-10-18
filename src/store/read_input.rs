//! Read-call pins outlive detached filter encoding, never the returned native cursor.
use pyo3::pybacked::PyBackedStr;

use super::typed_view::TermView;

pub(super) type TermTuple = (
    PyBackedStr,
    PyBackedStr,
    Option<PyBackedStr>,
    Option<PyBackedStr>,
);
pub(super) type Pattern = (Option<TermTuple>, Option<TermTuple>, Option<TermTuple>);

pub(super) fn view(source: &Option<TermTuple>) -> Option<TermView<'_>> {
    source.as_ref().map(term_view)
}

pub(super) fn term_view((kind, value, datatype, language): &TermTuple) -> TermView<'_> {
    TermView::new(
        kind.as_str(),
        value.as_str(),
        datatype.as_deref(),
        language.as_deref(),
    )
}
