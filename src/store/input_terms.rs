//! Caller-owned raw RDF terms: inspect Python first, pin text, then detach.
use pyo3::basic::CompareOp;
use pyo3::class::gc::{PyTraverseError, PyVisit};
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedStr;
use pyo3::types::{PyString, PyTuple, PyType};

use super::typed_view::TermView;

#[pyclass(frozen, module = "rdflib_turbo._native")]
pub(crate) struct NativeInputTerms {
    uri: Py<PyType>,
    bnode: Py<PyType>,
    literal: Py<PyType>,
    // bounded: five immutable attribute names for every call, never data-dependent.
    names: [Py<PyString>; 5],
}

#[pymethods]
impl NativeInputTerms {
    #[new]
    pub(super) fn new(
        py: Python<'_>,
        uri: Py<PyType>,
        bnode: Py<PyType>,
        literal: Py<PyType>,
    ) -> PyResult<Self> {
        for class in [&uri, &bnode, &literal] {
            if !class.bind(py).is_subclass_of::<PyString>()? {
                return Err(PyTypeError::new_err(
                    "Input term classes must be string subclasses",
                ));
            }
        }
        let names = [
            "datatype",
            "language",
            "store",
            "identifier",
            "default_identifier",
        ]
        .map(|name| PyString::new(py, name).unbind());
        Ok(Self {
            uri,
            bnode,
            literal,
            names,
        })
    }

    fn __traverse__(&self, visit: PyVisit<'_>) -> Result<(), PyTraverseError> {
        visit.call(&self.uri)?;
        visit.call(&self.bnode)?;
        visit.call(&self.literal)?;
        for name in &self.names {
            visit.call(name)?;
        }
        Ok(())
    }
}

// bounded: three optional Python handles per term, exactly four call-local terms.
struct RawTerm<'py> {
    // Match reverse tuple-field release order; text leases drop before these owners.
    language: Option<Bound<'py, PyAny>>,
    datatype: Option<Bound<'py, PyAny>>,
    value: Bound<'py, PyAny>,
    kind: &'static str,
}

// Reverse argument release order; no metadata is released during detached lookup.
pub(super) struct RawPattern<'py> {
    graph: Option<RawTerm<'py>>,
    object: Option<RawTerm<'py>>,
    predicate: Option<RawTerm<'py>>,
    subject: Option<RawTerm<'py>>,
}

pub(super) struct PinnedTerm {
    value: PyBackedStr,
    datatype: Option<PyBackedStr>,
    language: Option<PyBackedStr>,
    kind: &'static str,
}

pub(super) struct PinnedPattern {
    pub subject: PinnedTerm,
    pub predicate: PinnedTerm,
    pub object: PinnedTerm,
    pub graph: Option<PinnedTerm>,
}

impl NativeInputTerms {
    pub(super) fn stage_record<'py>(
        &self,
        py: Python<'py>,
        record: &Bound<'py, PyAny>,
    ) -> PyResult<RawPattern<'py>> {
        let record = record.cast::<PyTuple>()?;
        if record.len() != 4 {
            return Err(PyValueError::new_err(
                "An import record must contain exactly four terms",
            ));
        }
        let subject = self.classify(py, record.get_item(0)?)?;
        let predicate = self.classify(py, record.get_item(1)?)?;
        let object = self.classify(py, record.get_item(2)?)?;
        let graph = self.classify(py, record.get_item(3)?)?;
        Ok(RawPattern {
            graph,
            object,
            predicate,
            subject,
        })
    }

    pub(super) fn stage_graph(
        &self,
        py: Python<'_>,
        graph: Bound<'_, PyAny>,
    ) -> PyResult<Option<PinnedTerm>> {
        self.classify(py, graph)?
            .as_ref()
            .map(RawTerm::pin)
            .transpose()
    }

    fn classify<'py>(
        &self,
        py: Python<'py>,
        value: Bound<'py, PyAny>,
    ) -> PyResult<Option<RawTerm<'py>>> {
        if value.is_none() {
            return Ok(None);
        }
        let kind = if value.is_instance(self.uri.bind(py).as_any())? {
            "uri"
        } else if value.is_instance(self.bnode.bind(py).as_any())? {
            "bnode"
        } else if value.is_instance(self.literal.bind(py).as_any())? {
            "literal"
        } else {
            let name = value.get_type().getattr("__name__")?;
            let message = PyString::new(py, "Unsupported RDF term: {}")
                .call_method1("format", (name,))?
                .extract::<String>()?;
            return Err(PyTypeError::new_err(message));
        };
        let (datatype, language) = if kind == "literal" {
            let datatype = value.getattr(self.names[0].bind(py))?;
            let language = value.getattr(self.names[1].bind(py))?;
            (Some(datatype), Some(language))
        } else {
            (None, None)
        };
        Ok(Some(RawTerm {
            language,
            datatype,
            value,
            kind,
        }))
    }

    fn graph<'py>(
        &self,
        py: Python<'py>,
        context: &Bound<'py, PyAny>,
    ) -> PyResult<Option<RawTerm<'py>>> {
        // The Python encode_graph helper obtains its store argument after all SPO metadata.
        let store = context.getattr(self.names[2].bind(py))?;
        let identifier = if context.hasattr(self.names[3].bind(py))? {
            context.getattr(self.names[3].bind(py))?
        } else {
            context.clone()
        };
        let comparison = {
            let default = store.getattr(self.names[4].bind(py))?;
            identifier.rich_compare(&default, CompareOp::Eq)?
        };
        // Python releases COMPARE_OP operands before its following truth conversion.
        let is_default = comparison.is_truthy()?;
        drop(comparison);
        if is_default {
            Ok(None)
        } else {
            self.classify(py, identifier)
        }
    }

    pub(super) fn stage<'py>(
        &self,
        py: Python<'py>,
        pattern: &Bound<'py, PyAny>,
        context: &Bound<'py, PyAny>,
    ) -> PyResult<RawPattern<'py>> {
        let subject = self.classify(py, pattern.get_item(0)?)?;
        let predicate = self.classify(py, pattern.get_item(1)?)?;
        let object = self.classify(py, pattern.get_item(2)?)?;
        let graph = self.graph(py, context)?;
        Ok(RawPattern {
            graph,
            object,
            predicate,
            subject,
        })
    }
}

impl RawTerm<'_> {
    fn pin(&self) -> PyResult<PinnedTerm> {
        let value = self.value.extract()?;
        let datatype = self
            .datatype
            .as_ref()
            .filter(|value| !value.is_none())
            .map(|value| value.extract())
            .transpose()?;
        let language = self
            .language
            .as_ref()
            .filter(|value| !value.is_none())
            .map(|value| value.extract())
            .transpose()?;
        Ok(PinnedTerm {
            value,
            datatype,
            language,
            kind: self.kind,
        })
    }
}

impl RawPattern<'_> {
    pub(super) fn pin(&self, py: Python<'_>) -> PyResult<PinnedPattern> {
        fn required(py: Python<'_>, source: &Option<RawTerm<'_>>) -> PyResult<PinnedTerm> {
            match source {
                Some(term) => term.pin(),
                None => Err(py.None().bind(py).cast::<PyTuple>().unwrap_err().into()),
            }
        }
        // Python evaluates every encode before PyO3 extracts argument tuples in SPOG order.
        let subject = required(py, &self.subject)?;
        let predicate = required(py, &self.predicate)?;
        let object = required(py, &self.object)?;
        let graph = self.graph.as_ref().map(RawTerm::pin).transpose()?;
        Ok(PinnedPattern {
            subject,
            predicate,
            object,
            graph,
        })
    }
}

impl PinnedTerm {
    pub(super) fn view(&self) -> TermView<'_> {
        TermView::new(
            self.kind,
            self.value.as_str(),
            self.datatype.as_deref(),
            self.language.as_deref(),
        )
    }
}
