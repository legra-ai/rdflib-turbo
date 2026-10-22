//! Bounded, typed scans of one graph from an owned committed-store snapshot.
use std::sync::Mutex;

use oxigraph::model::{GraphNameRef, QuadRef};
use oxigraph::store::{LexicalQuadIter, LexicalTerm, Store};

use super::operations::runtime;
use super::session::lock;
use super::typed_view::{Position, PreparedTerm, TermView};
use crate::{Error, Result};

pub(super) type Row = (
    Option<LexicalTerm>,
    Option<LexicalTerm>,
    Option<LexicalTerm>,
);
pub(super) type FirstBatch = (Vec<Row>, Option<ReadCursor>);
const BATCH_ROWS: usize = 128;
const BATCH_BYTES: usize = 64 * 1024;
const INITIAL_ROWS: usize = 16;

struct CursorState {
    iter: Option<LexicalQuadIter<'static>>,
    pending: Option<Row>,
    capacity_hint: usize,
}

pub(super) struct ReadCursor {
    state: Mutex<CursorState>,
}

pub(super) fn start(
    store: &Store,
    subject: Option<TermView<'_>>,
    predicate: Option<TermView<'_>>,
    object: Option<TermView<'_>>,
    graph: Option<TermView<'_>>,
    mask: u8,
) -> Result<FirstBatch> {
    if mask & !7 != 0 {
        return Err(Error::Invalid(
            "Projection mask must be between 0 and 7".into(),
        ));
    }
    if mask == 0
        && let (Some(subject), Some(predicate), Some(object)) = (subject, predicate, object)
    {
        let found = contains_exact(store, subject, predicate, object, graph)?;
        // bounded: an exact quad lookup returns zero or one placeholder row.
        return Ok((
            if found {
                vec![(None, None, None)]
            } else {
                Vec::new()
            },
            None,
        ));
    }
    let pattern = PreparedRead::new(subject, predicate, object, graph)?;
    if !pattern.can_match() {
        return Ok((Vec::new(), None));
    }
    let cursor = open(store, &pattern, mask)?;
    let (rows, finished) = cursor.batch()?;
    Ok((rows, if finished { None } else { Some(cursor) }))
}

pub(super) fn contains_exact(
    store: &Store,
    subject: TermView<'_>,
    predicate: TermView<'_>,
    object: TermView<'_>,
    graph: Option<TermView<'_>>,
) -> Result<bool> {
    let pattern = PreparedRead::new(Some(subject), Some(predicate), Some(object), graph)?;
    if !pattern.can_match() {
        return Ok(false);
    }
    let subject = pattern.subject.as_ref().expect("bound subject");
    let predicate = pattern.predicate.as_ref().expect("bound predicate");
    let object = pattern.object.as_ref().expect("bound object");
    let graph = &pattern.graph;
    store
        .contains(QuadRef::new(
            subject.subject_ref(),
            predicate.predicate_ref(),
            object.term_ref(),
            graph.as_ref().map_or(GraphNameRef::DefaultGraph, |value| {
                value.subject_ref().into()
            }),
        ))
        .map_err(runtime)
}

// bounded: four optional caller-owned RDF term views per read operation.
struct PreparedRead<'a> {
    subject: Option<PreparedTerm<'a>>,
    predicate: Option<PreparedTerm<'a>>,
    object: Option<PreparedTerm<'a>>,
    graph: Option<PreparedTerm<'a>>,
}

impl<'a> PreparedRead<'a> {
    fn new(
        subject: Option<TermView<'a>>,
        predicate: Option<TermView<'a>>,
        object: Option<TermView<'a>>,
        graph: Option<TermView<'a>>,
    ) -> Result<Self> {
        // Read terms are valid RDF terms even when their positions cannot match.
        // Validate every term before deciding absence, including later metadata.
        Ok(Self {
            subject: subject
                .map(|v| PreparedTerm::new(v, Position::Object))
                .transpose()?,
            predicate: predicate
                .map(|v| PreparedTerm::new(v, Position::Object))
                .transpose()?,
            object: object
                .map(|v| PreparedTerm::new(v, Position::Object))
                .transpose()?,
            graph: graph
                .map(|v| PreparedTerm::new(v, Position::Subject))
                .transpose()?,
        })
    }

    fn can_match(&self) -> bool {
        self.subject
            .as_ref()
            .is_none_or(|v| !matches!(v.term_ref(), oxigraph::model::TermRef::Literal(_)))
            && self
                .predicate
                .as_ref()
                .is_none_or(|v| matches!(v.term_ref(), oxigraph::model::TermRef::NamedNode(_)))
    }
}

fn open(store: &Store, pattern: &PreparedRead<'_>, mask: u8) -> Result<ReadCursor> {
    let PreparedRead {
        subject,
        predicate,
        object,
        graph,
    } = pattern;
    let iter = store
        .quads_for_pattern(
            subject.as_ref().map(PreparedTerm::subject_ref),
            predicate.as_ref().map(PreparedTerm::predicate_ref),
            object.as_ref().map(PreparedTerm::term_ref),
            Some(graph.as_ref().map_or(GraphNameRef::DefaultGraph, |value| {
                value.subject_ref().into()
            })),
        )
        .project_lexical(mask & 1 != 0, mask & 2 != 0, mask & 4 != 0);
    Ok(ReadCursor {
        state: Mutex::new(CursorState {
            iter: Some(iter),
            pending: None,
            capacity_hint: INITIAL_ROWS,
        }),
    })
}

fn row([subject, predicate, object]: [Option<LexicalTerm>; 3]) -> Row {
    (subject, predicate, object)
}

fn row_bytes(row: &Row) -> usize {
    [&row.0, &row.1, &row.2]
        .into_iter()
        .flatten()
        .map(|term| match term {
            LexicalTerm::NamedNode(value) | LexicalTerm::BlankNode(value) => {
                64 + value.as_str().len()
            }
            LexicalTerm::Literal {
                value,
                datatype,
                language,
            } => {
                64 + value.as_str().len()
                    + datatype.as_ref().map_or(0, |text| text.as_str().len())
                    + language.as_ref().map_or(0, |text| text.as_str().len())
            }
        })
        .sum()
}

impl ReadCursor {
    pub(super) fn release(&self) -> Result<()> {
        let mut state = lock(&self.state)?;
        state.iter.take();
        state.pending.take();
        Ok(())
    }

    pub(super) fn batch(&self) -> Result<(Vec<Row>, bool)> {
        let mut state = lock(&self.state)?;
        // bounded: at most 128 rows plus one pending row; an atomic RDF row may exceed 64 KiB.
        let mut rows = Vec::new();
        let mut bytes = 0;
        while rows.len() < BATCH_ROWS {
            let next = match state.pending.take() {
                Some(row) => row,
                None => match state.iter.as_mut().and_then(Iterator::next) {
                    Some(Ok(terms)) => row(terms),
                    Some(Err(cause)) => {
                        state.iter.take();
                        return Err(runtime(cause));
                    }
                    None => {
                        state.iter.take();
                        break;
                    }
                },
            };
            let size = row_bytes(&next);
            if !rows.is_empty() && size > BATCH_BYTES - bytes {
                state.pending = Some(next);
                break;
            }
            bytes += size;
            if rows.len() == rows.capacity() {
                // bounded: geometric growth is explicitly capped at 128 slots.
                let target = if rows.is_empty() {
                    state.capacity_hint
                } else {
                    (rows.capacity() * 2).min(BATCH_ROWS)
                };
                rows.reserve_exact(target - rows.len());
            }
            rows.push(next);
            if bytes >= BATCH_BYTES {
                break;
            }
        }
        state.capacity_hint = rows.len().clamp(1, BATCH_ROWS);
        Ok((rows, state.iter.is_none()))
    }
}

#[cfg(test)]
#[path = "read_tests.rs"]
mod tests;
