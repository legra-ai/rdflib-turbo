# ADR 0004: Run SHACL while borrowing the owned dataset

Status: Accepted

Supersedes ADR 0001's initial RDFLib-copy boundary for SHACL. Keep dataset
ownership in this runtime: compile the canonical pyshacl-turbo engine as a
revision-pinned Git dependency in this extension. Retain the same exact-term
Oxigraph patches and engine feature flags. Cargo.lock and the source revision
identify the implementation; this is neither a copied engine fork nor an ABI
assumption between two independently built extensions.

The internal Python integration protocol is SHARED_SHACL_PROTOCOL=1. It exposes
immutable SharedShapes and report output to pyshacl-turbo. All Dataset and engine
objects remain inside this extension. Ordinary validation holds a read lock and
borrows the original Dataset. Only operations with active rules make a working
copy; inplace rule validation takes a write lock, stages all changes, constructs
the complete report, and commits after success. No evaluation caches survive a
call. Named-graph unions remain private evaluation views. Tests prohibit Python
data copying/conversion during validation, cover concurrent reuse, later data
mutation, exact blank-node identity, and failures after staged rule insertions.

Retain parsed namespace metadata atomically alongside document data at the
Python boundary. No failed parse updates namespace bindings. Inference and copy
preserve those bindings, preventing report-message differences caused by a lost
prefix. The public native dataset class and inference API remain compatible
with owlrl-turbo's dataset argument; its package constraint must permit this
new runtime release. This alpha does not claim the entire RDFLib API.

The embedded engine adds code/dependencies to the native wheel. Benchmarks must
measure actual installed wheels, import overhead and whole-process RSS; sharing
one data object is not a promise of lower memory for every workload.
