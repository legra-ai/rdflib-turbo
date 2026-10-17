//! Native storage and transaction handles, owned by this extension only.
mod operations;
pub(crate) mod output;
mod session;

use std::sync::{Arc, Mutex};

use oxigraph::store::Store;
use pyo3::prelude::*;

use crate::{error, format};
use operations::{Operation, Reply, runtime};
use session::{Control, Session, lock};

#[pyclass(frozen)]
pub(crate) struct NativeStore {
    control: Arc<Mutex<Control>>,
}

impl NativeStore {
    fn snapshot(&self) -> crate::Result<Store> {
        lock(&self.control)?.read()
    }

    fn mutate(&self, operation: Operation) -> crate::Result<()> {
        let session = Session::start(self.control.clone())?;
        session.execute(operation)?;
        session.finish(true)
    }
}

#[pymethods]
impl NativeStore {
    #[new]
    #[pyo3(signature = (path=None))]
    fn new(py: Python<'_>, path: Option<String>) -> PyResult<Self> {
        py.detach(|| {
            let store = match path {
                None => Store::new().map_err(runtime)?,
                Some(path) => {
                    #[cfg(feature = "persistence")]
                    {
                        Store::open(path).map_err(runtime)?
                    }
                    #[cfg(not(feature = "persistence"))]
                    {
                        let _ = path;
                        return Err(crate::Error::Unsupported(
                            "This build has no RocksDB persistence support".into(),
                        ));
                    }
                }
            };
            Ok(Self {
                control: Arc::new(Mutex::new(Control::new(store))),
            })
        })
        .map_err(error)
    }

    fn ensure_open(&self) -> PyResult<()> {
        self.snapshot().map(|_| ()).map_err(error)
    }

    fn close(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| lock(&self.control)?.close()).map_err(error)
    }

    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        py.detach(|| self.snapshot()?.len().map_err(runtime))
            .map_err(error)
    }

    fn transaction(&self, py: Python<'_>) -> PyResult<NativeTransaction> {
        py.detach(|| {
            Session::start(self.control.clone()).map(|session| NativeTransaction { session })
        })
        .map_err(error)
    }

    #[pyo3(signature = (source, rdf_format, base=None))]
    fn load(
        &self,
        py: Python<'_>,
        source: String,
        rdf_format: &str,
        base: Option<String>,
    ) -> PyResult<()> {
        let format = format(rdf_format)?;
        py.detach(|| {
            self.mutate(Operation::Load {
                source,
                format,
                base,
            })
        })
        .map_err(error)
    }

    #[pyo3(signature = (mode, destination=None))]
    fn infer(&self, py: Python<'_>, mode: String, destination: Option<String>) -> PyResult<()> {
        py.detach(|| self.mutate(Operation::Infer(mode, destination)))
            .map_err(error)
    }

    fn update(&self, py: Python<'_>, source: String) -> PyResult<()> {
        py.detach(|| self.mutate(Operation::Update(source)))
            .map_err(error)
    }

    #[pyo3(signature = (source, default_union=false))]
    fn query(
        &self,
        py: Python<'_>,
        source: &str,
        default_union: bool,
    ) -> PyResult<(String, String)> {
        py.detach(|| {
            let store = self.snapshot()?;
            let (query, kind) = output::prepare_query(source, default_union)?;
            output::query_output(query.on_store(&store).execute()?, kind)
        })
        .map_err(error)
    }

    fn dump(&self, py: Python<'_>, rdf_format: &str) -> PyResult<String> {
        let format = format(rdf_format)?;
        py.detach(|| output::dump(self.snapshot()?.iter(), format))
            .map_err(error)
    }
}

#[pyclass(frozen)]
pub(crate) struct NativeTransaction {
    session: Session,
}

#[pymethods]
impl NativeTransaction {
    fn commit(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.session.finish(true)).map_err(error)
    }

    fn rollback(&self, py: Python<'_>) -> PyResult<()> {
        py.detach(|| self.session.finish(false)).map_err(error)
    }

    fn __len__(&self, py: Python<'_>) -> PyResult<usize> {
        py.detach(|| match self.session.execute(Operation::Len)? {
            Reply::Count(value) => Ok(value),
            _ => unreachable!("native transaction reply type"),
        })
        .map_err(error)
    }

    #[pyo3(signature = (source, rdf_format, base=None))]
    fn load(
        &self,
        py: Python<'_>,
        source: String,
        rdf_format: &str,
        base: Option<String>,
    ) -> PyResult<()> {
        let format = match format(rdf_format) {
            Ok(format) => format,
            Err(cause) => {
                py.detach(|| self.session.finish(false)).map_err(error)?;
                return Err(cause);
            }
        };
        py.detach(|| {
            self.session
                .execute(Operation::Load {
                    source,
                    format,
                    base,
                })
                .map(|_| ())
        })
        .map_err(error)
    }

    #[pyo3(signature = (mode, destination=None))]
    fn infer(&self, py: Python<'_>, mode: String, destination: Option<String>) -> PyResult<()> {
        py.detach(|| {
            self.session
                .execute(Operation::Infer(mode, destination))
                .map(|_| ())
        })
        .map_err(error)
    }

    fn update(&self, py: Python<'_>, source: String) -> PyResult<()> {
        py.detach(|| self.session.execute(Operation::Update(source)).map(|_| ()))
            .map_err(error)
    }

    #[pyo3(signature = (source, default_union=false))]
    fn query(
        &self,
        py: Python<'_>,
        source: String,
        default_union: bool,
    ) -> PyResult<(String, String)> {
        py.detach(|| {
            match self
                .session
                .execute(Operation::Query(source, default_union))?
            {
                Reply::Query(kind, text) => Ok((kind, text)),
                _ => unreachable!("native transaction reply type"),
            }
        })
        .map_err(error)
    }

    fn dump(&self, py: Python<'_>, rdf_format: &str) -> PyResult<String> {
        let format = match format(rdf_format) {
            Ok(format) => format,
            Err(cause) => {
                py.detach(|| self.session.finish(false)).map_err(error)?;
                return Err(cause);
            }
        };
        py.detach(|| match self.session.execute(Operation::Dump(format))? {
            Reply::Text(text) => Ok(text),
            _ => unreachable!("native transaction reply type"),
        })
        .map_err(error)
    }
}

pub(crate) fn register(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<NativeStore>()?;
    module.add_class::<NativeTransaction>()?;
    Ok(())
}

#[cfg(test)]
mod session_tests;
