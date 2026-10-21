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

Retain parsed namespace metadata alongside document data under the same native
lock. Validation reads bindings under its data read lock, rather than accepting
a separately captured Python metadata snapshot. No failed parse updates namespace bindings. Inference and copy
preserve those bindings, preventing report-message differences caused by a lost
prefix. The public native dataset class and inference API remain compatible
with owlrl-turbo's dataset argument; its package constraint must permit this
new runtime release. This alpha does not claim the entire RDFLib API.

The embedded engine adds code/dependencies to the native wheel. Benchmarks must
measure actual installed wheels, import overhead and whole-process RSS; sharing
one data object is not a promise of lower memory for every workload.

Canonical engine changes must update this runtime's immutable dependency pin and
reported engine revision together. Coordinated benchmark source pins must select
that consumer revision; a newer pyshacl wheel cannot share an older embedded
engine. Keep the fingerprint check strict and test the actual wheel combination.
The installed-wheel revision regression also checks the reported revision against
the declared Cargo source, preventing metadata from silently drifting from code.

Python warning policy is part of successful validation. Rule evaluation reuses
its existing working copy, but an operation with recursion warnings must release
the dataset lock before invoking Python's warning machinery. The provider emits
the public `pyshacl_turbo.ShapeRecursionWarning` category at the same caller
location as ordinary Graph validation and returns no deferred warning messages.
Warnings promoted to errors discard the staged data and namespace metadata.
Python warning callbacks may read or mutate the receiver without deadlocking.

The unlocked warning interval uses an optimistic owner-local mutation revision.
Every successful mutator, including metadata-only and no-op writes, advances a
checked revision; failed parsing and inference roll back before returning.
Independent copies start independent revisions. Accepted warnings are followed
by one compare-and-commit under the write lock. Any intervening write causes a
hard transaction-conflict error: preserve that write, discard the outer staged
rule changes, and never retry Python callbacks. Revision exhaustion fails before
mutation. Validation without warnings retains its original single lock and
working-copy cost; no additional dataset snapshot is introduced.

`SharedShapes.expand(dataset, settings, inplace)` runs the pinned engine's rule
expansion directly, without validation evaluation, report formatting or report
term conversion. It shares the strict settings boundary and warning-before-commit
policy with validation. Inplace expansion returns `(None, [])`; nonmutating
expansion returns a newly owned native dataset and an empty warning list. The
consumer retains the original inplace wrapper or wraps the new native owner.
Names, graph contexts, exact terms and namespace metadata remain native.

Active inplace rules stage one working state for rollback and the same owner
revision comparison used by validation. A successful no-op inplace expansion
still advances its owner's revision. When no selected rule is active, the engine
validates options under the write lock without copying graph data. Nonmutating
expansion necessarily owns its complete output, starts an independent revision,
and does not snapshot the input again. Rule-only expansion never constructs a
validation report. This extends protocol 1 using an existing pinned engine API;
its source revision and engine fingerprint do not change.
