//! Python term factories that materialize native rows as rdflib terms.
use oxigraph::store::{LexicalGraphName, LexicalQuad, LexicalTerm, LexicalUnionRow, TermText};
use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::exceptions::PyTypeError;
use pyo3::prelude::*;
use pyo3::types::{PyString, PyType};

use super::super::read;
use super::pointer_batch::PositionCache;

pub(super) type Row = (Option<Py<PyAny>>, Option<Py<PyAny>>, Option<Py<PyAny>>);
pub(in crate::store) type QuadRow = (Py<PyAny>, Py<PyAny>, Py<PyAny>, Option<Py<PyAny>>);
const MIN_REUSE_ROWS: usize = 8;

#[pyclass(frozen, module = "rdflib_turbo._native")]
pub(crate) struct NativeTermFactories {
    uri: Py<PyAny>,
    bnode: Py<PyAny>,
    literal: Py<PyAny>,
    allocate: Py<PyAny>,
    reuse_identifiers: bool,
}

#[pymethods]
impl NativeTermFactories {
    #[new]
    #[pyo3(signature = (uri, bnode, literal, allocate, reuse_identifiers=false))]
    pub(in crate::store) fn new(
        py: Python<'_>,
        uri: Py<PyAny>,
        bnode: Py<PyAny>,
        literal: Py<PyAny>,
        allocate: Py<PyAny>,
        reuse_identifiers: bool,
    ) -> PyResult<Self> {
        for class in [&uri, &bnode] {
            let class = class.bind(py).cast::<PyType>()?;
            if !class.is_subclass_of::<PyString>()? {
                return Err(PyTypeError::new_err(
                    "Identifier factories must be string subclasses",
                ));
            }
        }
        for callable in [&literal, &allocate] {
            if !callable.bind(py).is_callable() {
                return Err(PyTypeError::new_err(
                    "Term restoration factories must be callable",
                ));
            }
        }
        Ok(Self {
            uri,
            bnode,
            literal,
            allocate,
            reuse_identifiers,
        })
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.uri)?;
        visit.call(&self.bnode)?;
        visit.call(&self.literal)?;
        visit.call(&self.allocate)
    }
}

impl NativeTermFactories {
    pub(in crate::store) fn union_rows(
        &self,
        py: Python<'_>,
        rows: Vec<LexicalUnionRow>,
        factories: &Py<Self>,
    ) -> PyResult<Vec<super::super::union_read::Row>> {
        let reuse = self.reuse_identifiers && rows.len() >= MIN_REUSE_ROWS;
        let mut caches = std::array::from_fn::<_, 3, _>(|_| PositionCache::default());
        let mut restore = |term, position| {
            if reuse {
                self.cached_term(py, Some(term), &mut caches[position])
            } else {
                self.term(py, Some(term))
            }
        };
        rows.into_iter()
            .map(|row| {
                let [s, p, o] = row.terms;
                let statement = (
                    restore(s, 0)?.expect("full subject"),
                    restore(p, 1)?.expect("full predicate"),
                    restore(o, 2)?.expect("full object"),
                );
                Ok((
                    statement,
                    super::super::context_read::NativeContextCursor::lazy(
                        row.contexts,
                        factories.clone_ref(py),
                    ),
                ))
            })
            .collect()
    }

    pub(in crate::store) fn graph_rows(
        &self,
        py: Python<'_>,
        rows: Vec<LexicalGraphName>,
    ) -> PyResult<Vec<Option<Py<PyAny>>>> {
        rows.into_iter()
            .map(|graph| self.term(py, graph.into_term()))
            .collect()
    }

    pub(in crate::store) fn quad_rows(
        &self,
        py: Python<'_>,
        rows: Vec<LexicalQuad>,
    ) -> PyResult<Vec<QuadRow>> {
        let reuse = self.reuse_identifiers && rows.len() >= MIN_REUSE_ROWS;
        // bounded: four positions with four <=256-byte identifiers per input batch.
        let mut caches = std::array::from_fn::<_, 4, _>(|_| PositionCache::default());
        let mut restore = |value, position| {
            if reuse {
                self.cached_term(py, value, &mut caches[position])
            } else {
                self.term(py, value)
            }
        };
        rows.into_iter()
            .map(|row| {
                Ok((
                    restore(Some(row.subject), 0)?.expect("full subject term"),
                    restore(Some(row.predicate), 1)?.expect("full predicate term"),
                    restore(Some(row.object), 2)?.expect("full object term"),
                    restore(row.graph_name.into_term(), 3)?,
                ))
            })
            .collect()
    }

    fn term(&self, py: Python<'_>, source: Option<LexicalTerm>) -> PyResult<Option<Py<PyAny>>> {
        let Some(source) = source else {
            return Ok(None);
        };
        let term = match source {
            LexicalTerm::NamedNode(value) => self
                .allocate
                .bind(py)
                .call1((self.uri.bind(py), value.as_str()))?,
            LexicalTerm::BlankNode(value) => self
                .allocate
                .bind(py)
                .call1((self.bnode.bind(py), value.as_str()))?,
            LexicalTerm::Literal {
                value,
                datatype,
                language,
            } => self.literal.bind(py).call1((
                value.as_str(),
                datatype.as_ref().map(TermText::as_str),
                language.as_ref().map(TermText::as_str),
            ))?,
        };
        Ok(Some(term.unbind()))
    }

    fn cached_term(
        &self,
        py: Python<'_>,
        source: Option<LexicalTerm>,
        cache: &mut PositionCache,
    ) -> PyResult<Option<Py<PyAny>>> {
        let key = match &source {
            Some(LexicalTerm::NamedNode(text)) => Some((false, text)),
            Some(LexicalTerm::BlankNode(text)) => Some((true, text)),
            _ => None,
        };
        let key = key.filter(|(_, text)| cache.eligible(text));
        let retained = if let Some((kind, text)) = key {
            if let Some(value) = cache.get(py, kind, text) {
                return Ok(Some(value));
            }
            cache.active().then(|| (kind, text.clone()))
        } else {
            None
        };
        let value = self.term(py, source)?;
        if let (Some((kind, text)), Some(value)) = (retained, &value) {
            cache.insert(py, kind, text, value);
        }
        Ok(value)
    }

    pub(super) fn rows(&self, py: Python<'_>, rows: Vec<read::Row>) -> PyResult<Vec<Row>> {
        // bounded: the native reader caps every input batch at 128 rows / ~64 KiB.
        // Avoid cache setup for short selective scans; byte-limited large batches still qualify.
        if self.reuse_identifiers && rows.len() >= MIN_REUSE_ROWS {
            // bounded: three positions, four identifiers <=256 UTF-8 bytes each, batch-local only.
            let mut caches = std::array::from_fn::<_, 3, _>(|_| PositionCache::default());
            return rows
                .into_iter()
                .map(|(s, p, o)| {
                    Ok((
                        self.cached_term(py, s, &mut caches[0])?,
                        self.cached_term(py, p, &mut caches[1])?,
                        self.cached_term(py, o, &mut caches[2])?,
                    ))
                })
                .collect();
        }
        rows.into_iter()
            .map(|(s, p, o)| Ok((self.term(py, s)?, self.term(py, p)?, self.term(py, o)?)))
            .collect()
    }
}
