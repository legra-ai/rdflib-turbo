# ADR0063: Use the host interpreter's Core regex authority

Status: Accepted in source; installed artifact validation pending

Extends ADR0020's CPython-specific wheel boundary and ADR0068's single native
provider. This decision covers Core `sh:pattern`; SPARQL REGEX/REPLACE remains
separately scoped. It does not establish full Python exception or lazy-error
activation parity.

Rust's Unicode classes, case tables and names can be newer than the actual
CPython host. Successful native matches can therefore disagree with pySHACL.
An immutable host policy belongs to the provider's SHACL module. Build its
compressed digit/word/space ranges, simple lowercase runs and finite CPython
case exceptions once per interpreter. Record the host's empty-input nonboundary
semantics explicitly. Do not reconstruct tables for individual stores/patterns
or retain a full scalar-domain array.

Every Python compilation entrypoint requires the module's typed frozen
`_REGEX_POLICY` authority as an explicit private keyword. Thin facades re-export
and pass the exact same object. Verify the current interpreter before detaching.
There is no implicit Rust Unicode authority when a Python caller omits it.
Standalone Rust compilation and the separately scoped SPARQL path retain their
explicit existing contract.

Compile literal/range case closure using compressed interval intersections and
the host's finite exceptions. Union category predicates after folding literals,
then apply negation. Category membership is not literal case closure:
IGNORECASE `[\w]` excludes the combining U+0345 even though `[ι]` includes it.
The native VM retains an immutable scalar mapping for Unicode backreferences;
backreferences use simple lowercase only. Scoped ASCII comparisons never use
Unicode folding or the larger literal/set case closure.

Named escapes resolve through the host `unicodedata._ucnhash_CAPI`, whose exact
CPython3.11–3.15 headers use a two-callback layout. Validate the capsule name,
nonnull callbacks and supported interpreter version while attached. Retain both
capsule and originating module throughout detached compilation, including on
worker threads; older interpreters free the callback structure with the capsule.
The audited `getcode` path uses immutable tables and sets no Python exception.
Pass `with_named_seq=0` so scalar aliases remain valid while sequences fail.

Validate identifiers at their original translation position using public
`PyUnicode_IsIdentifier` on a fresh exact Unicode object while attached. The
private XID scalar exports are not available consistently across these Python
versions. Reject an invalid identifier before later pattern errors; do not
preparse the graph, recompile it or defer identifier errors until afterward.
Alias validated capture names internally to ASCII so the delegate cannot impose
another Unicode identifier version. A temporary borrowed native resolver and
its owner leases are not retained in compiled IR. Unexpected host errors retain
their original Python exception and cannot be accepted as successful compilation.

Matching, captures and validation execute natively. No Python re compilation,
matcher fallback, per-match GIL callback or mutable process-global policy is
permitted. The opt-in matcher fork has a compiled implementation identity over
its complete Rust source, manifest, build helper and semantic feature selection.
The engine includes that identity alongside its other semantic forks; independent
source verification must derive the same seven-fork identity. Whole original
source archives and wheel payloads remain separate mandatory provenance checks.

Source regressions compare two deliberately different host policies, ordinary
CPython predicates and `_sre` mappings, identifier error precedence, off-thread
lookup, owner removal, invalid capsules, exact failure identity and ASCII versus
Unicode backreferences. Installed differential tests compare complete reports
and lexical accept/reject against the original packages. Native ValueError
versus host PatternError type/message/position and lazy activation remain
explicit follow-up obligations; these tests do not erase them.
