# ADR 0031: Inspect raw membership terms before native lookup

Status: Accepted

Fully bound ordinary Graph membership previously built Python transport tuples
for each term before the native boolean lookup. The caller already owns the
identifiers and literal metadata; constructing another transport shape repeats
work without contributing to exact quad lookup.

Use an immutable NativeInputTerms descriptor owning the three validated string
subclasses and five fixed Python attribute names. The standard adapter creates
one descriptor and passes the original pattern and Graph to contains_pattern.
Keep contains_terms for callers that already supply typed transport terms; Graph
membership does not select between the two paths or retry on an error. Existing
partial-pattern, property-path, override and transaction dispatch guards remain.
Identifier output batching is independent and unchanged.

Inspect pattern slots, classify terms and fetch literal metadata in their existing
Python callback order. Resolve Graph identity with the existing attribute and
rich-comparison semantics. Stage all Python metadata before extracting Unicode
leases, including metadata whose extraction later fails. Drop comparison operands
before truth conversion as Python comparison evaluation does; finalizers and
reentry are observable. Preserve original exceptions and stop immediately.

Pin caller-owned Unicode with PyBackedStr and perform the same shared validation
and exact native lookup while detached. No Python-backed values enter storage or
transaction workers. Python callbacks and reference destruction occur outside
native storage locks. Descriptor GC traversal visits its actual owned references;
there is no global mutable state, graph mirror, term cache or retained request.

Installed-wheel regressions cover transport avoidance, callback/error ordering,
comparison finalizers, metadata errors, reentrant mutation/close, Unicode,
stateful pattern access, descriptor validation and reference cycles. A serialized
CI allocation budget calls the actual Python binding with a prebuilt large
identifier and requires boolean presence/absence without lexical copies or rows.
The budget does not imply zero Python allocation, zero native hashing or universal
performance parity.
