//! Checked attached source extraction; no host conversion is repeated.
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyString, PyType};
use pyshacl_turbo_engine::oxigraph::model::*;

pub(super) const BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
pub(super) struct Factories<'py> {
    uri: Bound<'py, PyType>,
    blank: Bound<'py, PyType>,
    literal: Bound<'py, PyType>,
}
pub(super) struct Row {
    pub quad: Quad,
    pub boolean: bool,
}
impl<'py> Factories<'py> {
    pub fn new(py: Python<'py>, classes: &[Py<PyType>; 3]) -> PyResult<Self> {
        for class in classes {
            if !class.bind(py).is_subclass_of::<PyString>()? {
                return Err(PyTypeError::new_err(
                    "Input term classes must be string subclasses",
                ));
            }
        }
        Ok(Self::bind(py, classes))
    }
    pub fn bind(py: Python<'py>, classes: &[Py<PyType>; 3]) -> Self {
        Self {
            uri: classes[0].bind(py).clone(),
            blank: classes[1].bind(py).clone(),
            literal: classes[2].bind(py).clone(),
        }
    }
    pub fn python_term(&self, term: TermRef<'_>) -> PyResult<Bound<'py, PyAny>> {
        match term {
            TermRef::NamedNode(n) => self.uri.call1((n.as_str(),)),
            TermRef::BlankNode(n) => self.blank.call1((n.as_str(),)),
            TermRef::Literal(literal) => {
                // This is a compiled operand, not an attestation of an original
                // SHAPES receiver. Keep its lexical word without renormalizing.
                let kwargs = PyDict::new(self.literal.py());
                kwargs.set_item("normalize", false)?;
                if let Some(language) = literal.language() {
                    kwargs.set_item("lang", language)?;
                } else if literal.has_explicit_datatype() {
                    kwargs.set_item("datatype", self.uri.call1((literal.datatype().as_str(),))?)?;
                }
                self.literal.call((literal.value(),), Some(&kwargs))
            }
        }
    }
    pub fn stage(
        &self,
        value: &Bound<'py, PyAny>,
        nodes: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<(Term, bool)> {
        let text = value.str()?.to_str()?.to_owned();
        if value.is_instance(&self.uri)? {
            return Ok((
                NamedNode::new(text)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?
                    .into(),
                false,
            ));
        }
        if value.is_instance(&self.blank)? {
            if let Some(nodes) = nodes {
                nodes.set_item(&text, value)?;
            }
            return Ok((
                BlankNode::new(text)
                    .map_err(|e| PyValueError::new_err(e.to_string()))?
                    .into(),
                false,
            ));
        }
        if !value.is_instance(&self.literal)? {
            return Err(PyTypeError::new_err(
                "Expected an RDFLib IRI, blank node or literal",
            ));
        }
        let language = value.getattr("language")?;
        let datatype = value.getattr("datatype")?;
        let datatype = if datatype.is_none() || !datatype.is_truthy()? {
            None
        } else {
            Some(datatype.str()?.to_str()?.to_owned())
        };
        let observation = matches!(text.as_str(), "true" | "false")
            && datatype.as_deref() == Some(BOOLEAN)
            && language.is_none();
        let term = if !language.is_none() {
            Literal::new_language_tagged_literal(text, language.extract::<String>()?)
                .map_err(|e| PyValueError::new_err(e.to_string()))?
        } else if let Some(datatype) = datatype {
            Literal::new_typed_literal(
                text,
                NamedNode::new(datatype).map_err(|e| PyValueError::new_err(e.to_string()))?,
            )
        } else {
            Literal::new_simple_literal(text)
        };
        Ok((term.into(), observation))
    }
    pub fn quad(
        &self,
        row: &Bound<'py, PyAny>,
        nodes: Option<&Bound<'py, PyDict>>,
        context: bool,
        default: Option<&Bound<'py, PyAny>>,
        flatten: bool,
    ) -> PyResult<Row> {
        let mut row = row.try_iter()?;
        let missing = || PyValueError::new_err("Invalid ordinary source row");
        let s = row.next().ok_or_else(missing)??;
        let p = row.next().ok_or_else(missing)??;
        let o = row.next().ok_or_else(missing)??;
        let graph = if context {
            Some(row.next().ok_or_else(missing)??)
        } else {
            None
        };
        if row.next().transpose()?.is_some() {
            return Err(missing());
        }
        let (s, _) = self.stage(&s, nodes)?;
        let (p, _) = self.stage(&p, nodes)?;
        let (o, boolean) = self.stage(&o, nodes)?;
        let subject: NamedOrBlankNode = match s {
            Term::NamedNode(n) => n.into(),
            Term::BlankNode(n) => n.into(),
            _ => {
                return Err(PyValueError::new_err(
                    "RDF subjects must be IRIs or blank nodes",
                ));
            }
        };
        let Term::NamedNode(predicate) = p else {
            return Err(PyValueError::new_err("RDF predicates must be IRIs"));
        };
        let graph = if !context
            || flatten
            || graph
                .as_ref()
                .expect("context graph")
                .eq(default.expect("context requires actual default"))?
        {
            GraphName::DefaultGraph
        } else {
            match self.stage(graph.as_ref().expect("context graph"), nodes)?.0 {
                Term::NamedNode(n) => n.into(),
                Term::BlankNode(n) => n.into(),
                _ => {
                    return Err(PyValueError::new_err(
                        "RDF graph names must be IRIs or blank nodes",
                    ));
                }
            }
        };
        Ok(Row {
            quad: Quad::new(subject, predicate, o, graph),
            boolean,
        })
    }
}
