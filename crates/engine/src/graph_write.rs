//! Rule writes stay on the caller-owned graph and retain only their delta.
use crate::{Error, GraphRead, Result};
use oxigraph::model::{Dataset, QuadRef};
use oxigraph::store::Transaction;

pub(crate) trait GraphWrite: GraphRead {
    fn insert_quad(&mut self, quad: QuadRef<'_>) -> Result<bool>;
}

impl GraphWrite for Dataset {
    fn insert_quad(&mut self, quad: QuadRef<'_>) -> Result<bool> {
        Ok(Dataset::insert(self, quad))
    }
}

impl GraphWrite for Transaction<'_> {
    fn insert_quad(&mut self, quad: QuadRef<'_>) -> Result<bool> {
        if self
            .contains(quad)
            .map_err(|error| Error::Runtime(format!("Storage read failed: {error}")))?
        {
            return Ok(false);
        }
        self.insert(quad);
        Ok(true)
    }
}
