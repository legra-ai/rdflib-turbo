//! Changed-only inference publication, including rare full-Term removals.
#[cfg(test)]
use oxigraph::model::GeneralizedQuad;
use oxigraph::model::{GeneralizedDataset, GeneralizedQuadRef, GraphName};
use oxigraph::store::Transaction;
use oxrdf::GeneralizedTransaction;

#[derive(Default)]
pub(crate) struct InferenceDelta {
    pub(super) additions: GeneralizedDataset,
    removals: Option<Box<GeneralizedDataset>>,
}

impl InferenceDelta {
    #[cfg(test)]
    pub(crate) fn from_additions(additions: GeneralizedDataset) -> Self {
        Self {
            additions,
            ..Self::default()
        }
    }

    pub(super) fn retain_removals(&mut self, removals: Option<Box<GeneralizedDataset>>) {
        self.removals = removals;
    }

    #[cfg(test)]
    pub(super) fn remove_source(&mut self, quad: GeneralizedQuad) {
        self.removals
            .get_or_insert_with(Default::default)
            .insert(&quad);
    }

    pub(super) fn is_removed(&self, quad: GeneralizedQuadRef<'_>) -> bool {
        self.removals
            .as_ref()
            .is_some_and(|data| data.contains(quad))
    }

    pub(crate) fn removals(&self) -> impl Iterator<Item = GeneralizedQuadRef<'_>> {
        self.removals.iter().flat_map(|data| data.iter())
    }

    pub(crate) fn additions(&self) -> impl Iterator<Item = GeneralizedQuadRef<'_>> {
        self.additions.iter()
    }

    pub(crate) fn apply_dataset(&self, transaction: &mut GeneralizedTransaction<'_>) {
        for quad in self.removals() {
            transaction.remove(quad);
        }
        for quad in self.additions() {
            transaction.insert(quad);
        }
    }

    pub(crate) fn apply_store(&self, transaction: &mut Transaction<'_>, graph: Option<&GraphName>) {
        for quad in self.removals() {
            transaction.remove_generalized(GeneralizedQuadRef {
                graph_name: graph.map_or(quad.graph_name, GraphName::as_ref),
                ..quad
            });
        }
        for quad in self.additions() {
            transaction.insert_generalized(GeneralizedQuadRef {
                graph_name: graph.map_or(quad.graph_name, GraphName::as_ref),
                ..quad
            });
        }
    }

    #[cfg(test)]
    pub(super) fn has_removals(&self) -> bool {
        self.removals.is_some()
    }
}
