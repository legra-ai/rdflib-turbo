//! The revisioned dataset state owned by one native dataset.
use std::ops::{Deref, DerefMut};

use oxigraph::model::*;

use crate::{Error, Result};

/// One runtime-owned exact-term dataset. Locks cover whole native operations.
#[derive(Clone, Default)]
pub(crate) struct DatasetState {
    pub(crate) data: GeneralizedDataset,
    pub(crate) namespaces: Vec<(String, String)>,
    pub(crate) revision: u64,
}

impl DatasetState {
    pub(crate) fn new(
        data: impl Into<GeneralizedDataset>,
        namespaces: Vec<(String, String)>,
    ) -> Self {
        Self {
            data: data.into(),
            namespaces,
            revision: 0,
        }
    }

    pub(crate) fn copy_owner(&self) -> Self {
        Self::new(self.data.clone(), self.namespaces.clone())
    }

    pub(crate) fn next_revision(&self) -> Result<u64> {
        self.revision
            .checked_add(1)
            .ok_or_else(|| Error::Runtime("Dataset mutation revision exhausted".into()))
    }

    pub(crate) fn bind(&mut self, entries: Vec<(String, String)>) {
        for (prefix, namespace) in entries {
            if let Some((_, value)) = self.namespaces.iter_mut().find(|(key, _)| key == &prefix) {
                *value = namespace;
            } else {
                self.namespaces.push((prefix, namespace));
            }
        }
    }
}

impl Deref for DatasetState {
    type Target = GeneralizedDataset;
    fn deref(&self) -> &GeneralizedDataset {
        &self.data
    }
}

impl DerefMut for DatasetState {
    fn deref_mut(&mut self) -> &mut GeneralizedDataset {
        &mut self.data
    }
}
