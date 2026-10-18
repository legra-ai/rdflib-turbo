# ADR 0010: Index namespace metadata in both directions

Status: Accepted

The rdfpandas roundtrip profile found 4,003 namespace-to-prefix lookups for
1,000 records. Each native-facade lookup scanned every binding. Reference RDFLib
Memory has a reverse index. This avoidable Python work dominated namespace
normalization even though RDF storage and pattern matching execute natively.

Make namespace metadata an owning object with prefix-to-namespace and
namespace-to-prefix indexes. Reads in either direction use dictionary lookups.
Binding, replacement, and override behavior update both indexes according to
the reference Store contract. Do not cache lookup results without binding
invalidation, mirror RDF statements in Python, or change application code.

All views over one Store share this metadata object. Explicit transactions clone
namespace metadata and retain both directions. Successful exit replaces both
indexes in the original shared object; abort leaves the original unchanged.
Only namespace metadata is copied, never the RDF dataset. Persistence of namespace
metadata remains outside the current contract under ADR 0009.

The regression first rejects namespace lookups that scan the supplied bindings.
Differential binding tests check both lookup directions against RDFLib Memory.
Commit/abort regressions verify both indexes through another graph view sharing
the same native Store. Separate installed-wheel benchmarks measure the actual
improvement; this change alone does not establish overall performance superiority.
