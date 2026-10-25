//! Shared bounded full-quad stream over one owning lexical reader.
use super::operations::runtime;
use super::read::term_bytes;
use crate::Result;
use oxigraph::store::{GeneralizedLexicalQuadRows, LexicalGraphName, LexicalQuad};

const BATCH_ROWS: usize = 128;
const BATCH_BYTES: usize = 64 * 1024;

pub(super) struct Stream {
    iter: Option<GeneralizedLexicalQuadRows<'static>>,
    pending: Option<LexicalQuad>,
}

impl Stream {
    pub(super) fn new(iter: oxigraph::store::GeneralizedQuadIter<'static>) -> Self {
        Self {
            iter: Some(iter.project_quad_lexical()),
            pending: None,
        }
    }

    pub(super) fn batch(&mut self) -> Result<(Vec<LexicalQuad>, bool)> {
        // bounded: 128 rows plus one pending row; one atomic row may exceed 64 KiB.
        let mut rows = Vec::new();
        let mut bytes = 0;
        while rows.len() < BATCH_ROWS {
            let row = match self.pending.take() {
                Some(row) => row,
                None => match self.iter.as_mut().and_then(Iterator::next) {
                    Some(row) => row.map_err(runtime)?,
                    None => {
                        self.iter.take();
                        break;
                    }
                },
            };
            let size = quad_bytes(&row);
            if !rows.is_empty() && size > BATCH_BYTES.saturating_sub(bytes) {
                self.pending = Some(row);
                break;
            }
            bytes += size;
            if rows.len() == rows.capacity() {
                // bounded: geometric growth is capped independently of statement count.
                let capacity = if rows.is_empty() {
                    16
                } else {
                    (rows.capacity() * 2).min(BATCH_ROWS)
                };
                rows.reserve_exact(capacity - rows.len());
            }
            rows.push(row);
            if bytes >= BATCH_BYTES {
                break;
            }
        }
        Ok((rows, self.iter.is_none()))
    }
}

pub(super) fn quad_bytes(row: &LexicalQuad) -> usize {
    term_bytes(&row.subject)
        + term_bytes(&row.predicate)
        + term_bytes(&row.object)
        + match &row.graph_name {
            LexicalGraphName::DefaultGraph => 0,
            LexicalGraphName::NamedNode(text) | LexicalGraphName::BlankNode(text) => {
                64 + text.as_str().len()
            }
        }
}
