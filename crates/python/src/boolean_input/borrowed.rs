//! One-row ordinary Graph cursor; all source work runs on the original caller.
use super::{caller::Caller, source::Factories};
use pyo3::{prelude::*, types::PyIterator};
use pyshacl_turbo_engine::{GraphRead, Result, oxigraph::model::*};
use std::sync::Arc;

// The generic indexed iterator's size is unknown here. Keep the bounded cursor
// on the stack rather than allocating a box for every selected graph read.
#[allow(clippy::large_enum_variant)]
pub(super) enum Rows<I> {
    Indexed(I),
    Borrowed(Cursor),
    Empty,
}
pub(super) fn rows<'a>(
    data: Option<&'a Dataset>,
    caller: Option<&Arc<Caller>>,
    s: Option<TermRef<'_>>,
    p: Option<TermRef<'_>>,
    o: Option<TermRef<'_>>,
    g: Option<GraphNameRef<'_>>,
) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a> {
    if let Some(data) = data {
        Rows::Indexed(data.matching_quads(s, p, o, g))
    } else if g.is_some_and(|name| !name.is_default_graph()) {
        Rows::Empty
    } else {
        Rows::Borrowed(Cursor {
            caller: caller.expect("borrowed source requires Caller").clone(),
            // bounded: exactly three optional pattern terms, independent of DATA size.
            pattern: [
                s.map(TermRef::into_owned),
                p.map(TermRef::into_owned),
                o.map(TermRef::into_owned),
            ],
            iterator: None,
            ended: false,
        })
    }
}
impl<I: Iterator<Item = Result<GeneralizedQuad>>> Iterator for Rows<I> {
    type Item = Result<GeneralizedQuad>;
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Indexed(rows) => rows.next(),
            Self::Borrowed(rows) => rows.next(),
            Self::Empty => None,
        }
    }
}

pub(super) struct Cursor {
    caller: Arc<Caller>,
    pattern: [Option<Term>; 3],
    iterator: Option<Py<PyIterator>>,
    ended: bool,
}
impl Iterator for Cursor {
    type Item = Result<GeneralizedQuad>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.ended {
            return None;
        }
        let result = self.caller.event(|py| {
            let factories = Factories::bind(py, &self.caller.classes);
            if self.iterator.is_none() {
                let pattern = self.pattern.each_ref().map(|term| {
                    term.as_ref().map_or_else(
                        || Ok(py.None()),
                        |term| factories.python_term(term.as_ref()).map(Bound::unbind),
                    )
                });
                let [s, p, o] = pattern;
                let rows = self
                    .caller
                    .view
                    .as_ref()
                    .expect("borrowed cursor requires its actual source view")
                    .bind(py)
                    .call_method1("triples", ((s?, p?, o?),))?;
                self.iterator = Some(rows.try_iter()?.unbind());
            }
            let mut iterator = self
                .iterator
                .as_ref()
                .expect("initialized iterator")
                .bind(py)
                .clone();
            iterator
                .next()
                .map(|row| {
                    factories
                        .quad(&row?, None, false, None, false)
                        .map(|row| row.quad.into())
                })
                .transpose()
        });
        match result {
            Ok(Some(row)) => Some(Ok(row)),
            Ok(None) => {
                self.ended = true;
                None
            }
            Err(error) => {
                self.ended = true;
                Some(Err(error))
            }
        }
    }
}
impl Drop for Cursor {
    fn drop(&mut self) {
        let mut iterator = self.iterator.take();
        if iterator.is_none() {
            return;
        }
        // A previously saved source error wins. Release the root attached even
        // when event refuses another callback. Like host iteration, decref the
        // iterator without inventing a call to an arbitrary close() method.
        // A generator's own finalizer runs attached and outside all mutexes.
        let _ = self.caller.event(|_| {
            drop(iterator.take());
            Ok(())
        });
        if iterator.is_some() {
            Python::attach(|_| drop(iterator));
        }
    }
}
