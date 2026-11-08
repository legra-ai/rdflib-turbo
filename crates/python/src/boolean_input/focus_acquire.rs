//! Actual source Core target sets, before any validation component reads values.
use super::{caller::Caller, focus::Payload, source::Factories};
use pyo3::{
    PyTypeInfo,
    prelude::*,
    types::{PyDict, PyList, PySet, PyString, PyTuple},
};
use pyshacl_turbo_engine::{SelectedTargetsRef, oxigraph::model::*};
use std::sync::Arc;

fn terms<'a, 'py>(
    py: Python<'py>,
    factories: &Factories<'py>,
    nodes: impl Iterator<Item = &'a Term>,
) -> PyResult<Bound<'py, PyList>> {
    let values = PyList::empty(py);
    for node in nodes {
        values.append(factories.python_term(node.as_ref())?)?;
    }
    Ok(values)
}
fn update(target: &Bound<'_, PySet>, values: &Bound<'_, PyAny>) -> PyResult<()> {
    target.call_method1("update", (values,))?;
    Ok(())
}
fn unpack_source_pair<'py>(
    py: Python<'py>,
    row: Bound<'py, PyAny>,
    python_unpack: &mut Option<Bound<'py, PyAny>>,
) -> PyResult<Bound<'py, PyTuple>> {
    if let Ok(pair) = row.cast_exact::<PyTuple>()
        && pair.len() == 2
    {
        return Ok(pair.clone());
    }
    let unpack = match python_unpack {
        Some(unpack) => unpack,
        None => {
            // The interpreter owns iterable consumption and its version-specific
            // errors. Compile once per acquisition, without a module or global cache.
            let namespace = PyDict::new(py);
            py.run(
                c"def unpack_pair(row):\n    subject, object = row\n    return subject, object\n",
                Some(&namespace),
                None,
            )?;
            let unpack = namespace
                .get_item("unpack_pair")?
                .expect("the local unpack function was just defined");
            // Do not retain a function -> globals -> function reference cycle.
            namespace.del_item("unpack_pair")?;
            python_unpack.insert(unpack)
        }
    };
    unpack
        .call1((row,))?
        .cast_into::<PyTuple>()
        .map_err(Into::into)
}
pub(super) fn acquire(
    py: Python<'_>,
    caller: &Caller,
    targets: SelectedTargetsRef<'_>,
    filter: Option<&[Term]>,
) -> PyResult<Payload> {
    let factories = Factories::bind(py, &caller.classes);
    let graph = caller.graph.bind(py);
    let found = PySet::empty(py)?;
    let nodes = terms(py, &factories, targets.nodes())?;
    update(&found, &nodes.call_method0("__iter__")?)?;
    let explicit = terms(py, &factories, targets.classes())?;
    let classes = PySet::type_object(py)
        .call1((explicit,))?
        .cast_into::<PySet>()?;
    let implicit = terms(py, &factories, targets.implicit_classes())?;
    let implicit = PySet::type_object(py)
        .call1((implicit,))?
        .cast_into::<PySet>()?;
    update(&classes, implicit.as_any())?;
    let instances = PySet::empty(py)?;
    let rdf_type = factories.python_term(
        NamedNodeRef::new_unchecked("http://www.w3.org/1999/02/22-rdf-syntax-ns#type").into(),
    )?;
    let subclass_of = factories.python_term(
        NamedNodeRef::new_unchecked("http://www.w3.org/2000/01/rdf-schema#subClassOf").into(),
    )?;
    let unique = PyDict::new(py);
    unique.set_item("unique", false)?;
    let remember = PyDict::new(py);
    remember.set_item("remember", py.None())?;
    for class in classes.iter() {
        let rows = graph.call_method("subjects", (&rdf_type, &class), Some(&unique))?;
        update(&instances, &rows)?;
        let subclasses = graph.call_method(
            "transitive_subjects",
            (&subclass_of, &class),
            Some(&remember),
        )?;
        for subclass in subclasses.try_iter()? {
            let subclass = subclass?;
            if subclass.eq(&class)? {
                continue;
            }
            let rows = graph.call_method("subjects", (&rdf_type, subclass), Some(&unique))?;
            update(&instances, &rows)?;
        }
    }
    update(&found, instances.as_any())?;
    let mut python_unpack = None;
    for (predicates, object_role) in [
        (targets.subjects_of().collect::<Vec<_>>(), false),
        (targets.objects_of().collect::<Vec<_>>(), true),
    ] {
        let all = PySet::empty(py)?;
        for predicate in predicates {
            let predicate = factories.python_term(predicate.as_ref().into())?;
            let rows = graph.call_method("subject_objects", (predicate,), Some(&unique))?;
            let selected = PySet::empty(py)?;
            for row in rows.try_iter()? {
                let pair = unpack_source_pair(py, row?, &mut python_unpack)?;
                let subject = pair.get_item(0)?;
                let object = pair.get_item(1)?;
                selected.add(if object_role { object } else { subject })?;
            }
            update(&all, selected.as_any())?;
        }
        update(&found, all.as_any())?;
    }
    let selected = PyList::type_object(py)
        .call1((&found,))?
        .cast_into::<PyList>()?;
    let selected = if let Some(filter) = filter {
        let permitted = terms(py, &factories, filter.iter())?;
        let filtered = PyList::empty(py);
        for object in selected.iter() {
            if object.is_instance(caller.classes[0].bind(py))? && permitted.contains(&object)? {
                filtered.append(object)?;
            }
        }
        filtered
    } else {
        selected
    };
    let mut words = Vec::with_capacity(selected.len());
    for object in selected.iter() {
        let word = if object.is_instance(caller.classes[0].bind(py))? {
            NamedNode::new(object.cast::<PyString>()?.to_str()?)
                .map(Term::from)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
        } else if object.is_instance(caller.classes[1].bind(py))? {
            BlankNode::new(object.cast::<PyString>()?.to_str()?)
                .map(Term::from)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?
        } else {
            factories.stage(&object, None)?.0
        };
        words.push(word);
    }
    Ok(Payload {
        _set: found.unbind(),
        selected: selected.unbind(),
        words: Arc::new(words),
    })
}
