//! Source-test-only canonical public pipeline; no wheel or performance claim.
use std::ffi::CString;

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyModule};

#[pyfunction]
fn memory_phase(name: &str, quads: usize) {
    super::phase(name, quads);
}

#[test]
#[ignore = "isolated requested-Rust-heap profile with frozen public Python source authorities"]
fn profile_canonical_store_pipeline_allocations() {
    Python::initialize();
    Python::attach(|py| {
        let native = PyModule::new(py, "source_memory_profile").unwrap();
        crate::_native(&native).unwrap();
        let globals = PyDict::new(py);
        globals.set_item("native", native).unwrap();
        globals
            .set_item("phase", wrap_pyfunction!(memory_phase, py).unwrap())
            .unwrap();
        globals
            .set_item("source_root", env!("CARGO_MANIFEST_DIR"))
            .unwrap();
        let source = CString::new(
            r#"
import importlib.util, json, os
from pathlib import Path
spec = importlib.util.spec_from_file_location('canonical_memory_fixture', Path(source_root) / 'scripts/canonical_memory_fixture.py')
fixture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixture)
inputs = json.loads(Path(os.environ['MEMORY_SOURCE_INPUTS']).read_text())
result = fixture.execute(native, phase, inputs, os.environ['MEMORY_DATA'], os.environ['MEMORY_SHAPES'], os.environ['MEMORY_INFERENCE'])
print('MEMORY_SEMANTICS ' + json.dumps(result, sort_keys=True))
"#,
        )
        .unwrap();
        py.run(&source, Some(&globals), None).unwrap();
    });
}
