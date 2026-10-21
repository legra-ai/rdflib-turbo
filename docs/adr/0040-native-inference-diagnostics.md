# ADR 0040: Publish native inconsistency diagnostics after successful inference

Status: Accepted

OWL-RL 7.6.2 emits standard RDF diagnostic nodes for inconsistent input. These
are ordinary output, distinct from storage/parser failures and from the native
model's explicit exclusion of generalized RDF literal subjects.

Collect exact diagnostic messages during initial datatype checks and indexed
relation checks. Retain only output-sized message state, never a second graph
or an equality relation. Newly discovered messages deduplicate by exact string.
Caller-supplied prior messages preserve their order and existing duplicates,
matching the mutable direct-Semantics error list. Inference remains native;
there is no Python reasoner fallback.

After all inference rounds and checks succeed, append a fresh blank node for
each message, with rdf:type ErrorMessage and the error literal in
`http://www.daml.org/2002/03/agents/agent-ont#`. These new nodes are not inputs to
the current invocation's reasoning. Publication shares the existing insertion
journal or owning storage transaction. Any execution failure publishes no new
diagnostics or namespace metadata.

A frozen NativeInferenceOutcome carries owned error_messages across the typed
native boundary. Native delta inference returns the new dataset and outcome
together. Public infer still returns its owner and infer_delta still returns a
Dataset; their last-inference metadata refers to the immutable outcome. A
failed invocation clears that wrapper's last outcome without mutating the
caller-owned prior-message list. A successful explicit-transaction outcome is
provisional and may survive as a historical report after rollback; it is never
published into the base owner on rollback. Dataset namespace binding happens
under its existing write lock; graph-adapter bindings use their transaction's
existing staged namespace store.

The diagnostic lexical profile follows the actual AltXSDToPYTHON converter,
separately from default RDFLib Literal.value semantics used by RDFS. Capture
required interpreter conversion configuration per invocation without global
mutable state. Distinguish reference ValueError (an inconsistency diagnostic)
from other conversion exceptions (an execution failure). Preserve reference
numeric bounds and lexical quirks through differential boundary tests rather
than substituting strict SHACL datatype validation. Capture integer digit limits, Decimal precision/rounding/exponent bounds,
clamp and relevant traps, and the running interpreter time-parser policy as
owned per-invocation data. Unicode classification uses one audited scalar-only
CPython boundary; it borrows no Python objects during detached execution.
XML acceptance uses roxmltree with differential minidom fixtures. Newly raised Decimal status flags are caller-visible even when conversion
traps become diagnostics or a later inference/storage failure aborts the graph.
Each invocation owns one fixed atomic signal bitset shared by its native lexical
configuration and a caller-side publisher. The native worker retains no Python
objects. The publisher pins the captured caller flags mapping and signal classes;
after native completion (including transaction failure joins), it sets only
raised entries to true. It never clears existing flags, replaces the caller
context, changes configuration, or rolls status flags back with RDF data.
All nine Decimal signals are represented; the current string conversions can
raise seven and do not synthesize DivisionByZero or FloatOperation. Detached
conversion, trapped errors, default cardinality comparisons, late failures and
all public owners are regression-tested separately from graph atomicity.

Reference compatibility includes observable rule quirks: AllDifferent and
AllDisjointProperties exclude the final list member; AllDisjointClasses does
not. Complementarity messages use the reference's class-valued element field.
These are covered explicitly rather than silently replaced by idealized rules.

Validation compares exact messages, ordinary graph isomorphism, fresh nodes,
destinations, namespace collisions, repeated/prior-message calls, and atomic
rollback. Storage faults remain hard execution errors and do not become RDF
diagnostic strings.

List diagnostics read each immutable inference round. Invocation-owned
first-edge witnesses retain at most one rdf:first and rdf:rest edge per touched
diagnostic list cell. This preserves RDFLib Memory first-insertion selection
when later equality rounds add aliases; missing edges remain discoverable.
The witnesses contain schema traversal state, not ordinary graph/equality
relations. Restartable cursors and cycle detection avoid materializing every
pairwise list comparison. Built-in restriction_typing_check always returns true; subclass-provided
Python callbacks are outside this native built-in rule profile.

Initially multi-valued list edges remain order-dependent: native indexed
storage cannot reconstruct RDFLib Memory insertion order from an unordered
source. This change retains the native initial selection and records that
parity gap; it introduces no new rejection of reference-accepted input and
does not claim complete list-order parity.

Scoped Graph and StoreDataset inference binds the reference `err` namespace only
when the invocation emitted diagnostics, using RDFLib's destination namespace
collision rules. Implicit calls reuse their existing public transaction boundary:
native RDF writes remain pending while the staged graph binds its namespace.
Only successful commit replaces owner namespace metadata and publishes its last
outcome. Explicit transaction views expose provisional bindings and outcomes;
rollback or a caught binding failure cannot publish them to the original owner.
Only namespace metadata is copied, never input triples. This deliberately uses
an owner session for implicit scoped inference so Python namespace failures can
abort the native writes; no performance improvement is claimed for that change.
