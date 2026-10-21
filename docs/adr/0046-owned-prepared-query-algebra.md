# ADR 0046: Execute owned prepared RDFLib algebra

Status: Initial implementation; additional operators remain explicit unsupported cases

Prepared RDFLib Query objects carry resolved prologue/algebra and observable
execution metadata. Original text is neither authoritative nor always present.
RDFLib's translateAlgebra mutates EXISTS expressions, drops OPTIONAL guards and
cannot represent all query forms. Neither text reconstruction nor Python query
evaluation is an acceptable execution path.

Snapshot the caller's current Query object into a typed owned native plan. Reuse
the exact RDF term transport. Preserve supplied Join.lazy and OPTIONAL source
variables, including absent versus empty metadata. Read the actual attributes,
including explicit caller overrides, instead of assuming mapping entries remain
authoritative. Raw EXISTS syntax and already translated EXISTS algebra are
separate accepted input shapes; snapshotting never mutates the caller.

The native evaluator owns an immutable prepared plan and its structurally derived
source algebra. Query form, construction template, dataset clauses and base IRI
remain actual typed query fields. Execution rejects a source tree different from
the plan's source instead of recomputing metadata or selecting another plan.
Only basic graph pattern leaves receive index planning. Prepared plans are per
invocation and never enter an object-identity cache or copy input storage.

The initial slice handles SELECT, ASK, CONSTRUCT and DESCRIBE with basic graph
patterns, joins, OPTIONAL, projection, filters, EXISTS, inline values, ordering
and slicing. Remaining expression, aggregate and property-path variants fail
explicitly until their own native regressions pass. This is partial prepared API
coverage, not a claim of complete operator parity.

Native same-binary tests precede publication. Vendor evaluator changes require
matching canonical engine source and companion wheels before shared SHACL
integration tests can certify a release pair.

Prepared query blank nodes are binding keys shared by the owned query's BGPs,
including nested EXISTS patterns. A per-invocation lowering context replaces
these keys with fresh native variables; existing projection and join operators
continue to determine which bindings enter each subquery. Fresh names avoid the
complete source variable census, CONSTRUCT template variables and all initial
parameter names. Template blanks, expression constants and constant graph names
are not query binding keys and are never rewritten by this pass.

Source scope metadata retains key types. A Variable and a BNode with the same
spelling are different keys; plain strings and RDF constants do not match either
binding-key type. Absent and explicitly empty scopes remain distinct. The native
boundary requires tagged keys, rejects malformed shapes, and lowers nonmatching
keys to fresh unbound names rather than aliasing user variables. Initial binding
JSON is parsed once and shared between freshness analysis and parameter setup.
