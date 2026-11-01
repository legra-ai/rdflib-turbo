//! Attached, depth-first predicate sequences over actual Python owner sets.
use super::{Acquisition, frame::Frame, source};
use pyo3::{PyTypeInfo, prelude::*, types::PySet};
use pyshacl_turbo_engine::oxigraph::model::*;
use pyshacl_turbo_engine::{Error, Result, SelectedPathRef, SelectedTerm};

pub(super) fn acquire(
    frame: &Frame,
    focus: &Term,
    path: SelectedPathRef<'_>,
    subject: Option<pyshacl_turbo_engine::SelectedSubjectRef<'_>>,
) -> Result<Vec<SelectedTerm>> {
    let caller = frame.caller()?;
    let inherited = super::subject::Issued::resolve(&caller, subject)?;
    let values = caller.event(|py| {
        {
            let frames = caller.frames.lock().expect("source frames lock poisoned");
            frame.index(&frames)?;
        }
        let factories = source::Factories::bind(py, &caller.classes);
        let s = if let Some(subject) = inherited {
            subject.object(py, &caller)?.into_bound(py)
        } else {
            factories.python_term(focus.as_ref())?
        };
        let selected = walk(
            caller.graph.bind(py),
            &s,
            path.predicates(),
            &factories,
            caller.acquisition,
            0,
        )?;
        selected
            .map(|set| frame.stage_selected(&caller, &set, &factories))
            .transpose()
    })?;
    // This is an engine traversal limit, not a source-raised Python exception.
    // Keep its existing public ReportableRuntimeError translation distinct.
    values.ok_or_else(|| Error::Runtime("Path traversal depth is too much!".into()))
}

fn walk<'py, 'a>(
    graph: &Bound<'py, PyAny>,
    subject: &Bound<'py, PyAny>,
    mut predicates: impl ExactSizeIterator<Item = &'a NamedNode> + Clone,
    factories: &source::Factories<'py>,
    acquisition: Acquisition,
    depth: usize,
) -> PyResult<Option<Bound<'py, PySet>>> {
    // Original checks each remaining RDF-list node at recursion >= 10. URI
    // leaves bypass that check: ten predicates run, the eleventh does not.
    // bounded: at most eleven Rust frames, from the observed source boundary.
    if depth >= 10 {
        return Ok(None);
    }
    let predicate = predicates.next().expect("validated nonempty path suffix");
    let p = factories.python_term(predicate.as_ref().into())?;
    let selected = match acquisition {
        Acquisition::GraphObjects | Acquisition::PreparedDatasetObjects => {
            let rows = graph.call_method1("objects", (subject, p))?;
            // Original RdfLibDataGraph.objects delegates with yield-from, so an
            // overridden objects returning an existing set is iterated first.
            let iterator = rows.try_iter()?;
            PySet::type_object(graph.py())
                .call1((iterator,))?
                .cast_into::<PySet>()?
        }
        Acquisition::LegacyUnion => {
            // Keep the separately selected legacy store-union route explicit.
            let rows = graph.getattr("store")?.call_method1(
                "triples",
                ((subject, p, graph.py().None()), graph.py().None()),
            )?;
            let found = PySet::empty(graph.py())?;
            for row in rows.try_iter()? {
                found.add(row?.get_item(0)?.get_item(2)?)?;
            }
            found
        }
    };
    if predicates.len() == 0 {
        return Ok(Some(selected));
    }
    let found = PySet::empty(graph.py())?;
    for value in selected.iter() {
        let Some(branch) = walk(
            graph,
            &value,
            predicates.clone(),
            factories,
            acquisition,
            depth + 1,
        )?
        else {
            return Ok(None);
        };
        // Exact Python set.update reuses stored hashes and preserves its own
        // equality/owner selection; Rust must not rededuplicate these values.
        found.call_method1("update", (branch,))?;
    }
    Ok(Some(found))
}
