//! One borrowed RDF owner and an explicit optional source-event authority.
use super::caller::Caller;
use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::{GraphRead, Result, SelectedValueSource};
use std::sync::Arc;

pub(crate) struct Captured<'a> {
    pub data: &'a Dataset,
    pub caller: Option<Arc<Caller>>,
}
pub(super) fn word(term: TermRef<'_>) -> Option<bool> {
    match term {
        TermRef::Literal(l) if l.datatype().as_str() == super::source::BOOLEAN => match l.value() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    }
}
impl<'source> GraphRead for Captured<'source> {
    fn selected_focus_source(&self) -> Option<&dyn pyshacl_turbo_engine::SelectedFocusSource> {
        self.caller
            .as_deref()
            .filter(|c| !matches!(c.acquisition, super::Acquisition::LegacyUnion))
            .map(|caller| caller as &dyn pyshacl_turbo_engine::SelectedFocusSource)
    }
    fn selected_value_source(&self) -> Option<&dyn SelectedValueSource> {
        self.caller
            .as_deref()
            .filter(|c| c.values_admitted)
            .map(|caller| caller as &dyn SelectedValueSource)
    }
    fn boolean_object_state(
        &self,
        _: &GeneralizedQuad,
    ) -> Result<Option<pyshacl_turbo_engine::BuiltinBooleanState>> {
        // Live selected authority is never guessed from absent serialized metadata.
        Ok(None)
    }
    fn matching_quads<'a>(
        &'a self,
        s: Option<TermRef<'_>>,
        p: Option<TermRef<'_>>,
        o: Option<TermRef<'_>>,
        g: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'source> {
        self.data.matching_quads(s, p, o, g)
    }
    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        self.data.named_graphs()
    }
    fn contains_named_graph(&self, g: &NamedOrBlankNode) -> Result<bool> {
        self.data.contains_named_graph(g)
    }
}
