# ADR 0066: Compile full-Term shapes from their owned indexed source

Status: Accepted for implementation; compiler integration and installed artifact validation pending.

Original RDFLib shapes graphs can contain literal shape identifiers, generalized
metadata and nonstandard programmatic identifier spellings. A strict syntax
Dataset and text serialization cannot carry that source unchanged. The native
generalized relation already supports these terms in its dictionary, existing
indexes and owned immutable read versions (ADR 0065).

Widen GraphRead subjects and predicates to Term, returning generalized quads.
Native snapshots, transactions, query tuples and composed views must preserve
every position and propagate read failures. The legacy strict Dataset adapter
can return no matches for a bound term outside its representable domain; native
sources must never silently narrow or omit such rows. Structural SHACL path and
parameter checks retain their independent domain constraints.

IndexedShapesSource retains the actual ReadSnapshot and selected physical
context. A selected default, named or blank context becomes the sole logical
default graph. An absent selection exposes the dataset union as logical default
and preserves named graph placement and empty graphs. Reuse the existing indexed
scoped/union views and their indexed representative checks. Do not allocate a
second relation, copied base Dataset, side index or skolem encoding. An owned
snapshot survives mutation or closure of the originating caller's Store.
Pending transaction compilation must fail explicitly until an immutable pending
version can be retained with the same ownership guarantees.

The compiler will own this source. Syntax input must stream directly into its
indexed relation. Programmatic shapes use a typed full-Term input boundary,
preserving RDFLib URIRef/blank lexical acceptance without relaxing syntax parsing
or the ordinary data decoder. Canonicalization validates every record before
cache lookup and constructs no Store, Dataset or index on a hit. On a miss,
borrowed records populate the retained source once. Cache identity covers every
consumed quad, logical scope and namespace; incidental random identifiers on
contextless Graphs must not defeat reuse when they are not consumed semantics.

The numeric-boolean compiler policy must be a read projection over the same
indexes. Rewrite exact lexical "1" only in the five existing SHACL boolean roles
when that policy disables numeric booleans. Use at most two indexed streams and
indexed point checks to suppress projected duplicates. Preserve the original
source for reports; do not clone and rewrite all shapes.

Acceptance requires differential original/native reports, literal/URI/blank
shape controls, selected context and Dataset placement tests, owner lifetime,
advanced query/path/boolean regressions and independently verified installed
artifacts. Original Dataset bootstrap writes and default_union side effects are
separate observable wrapper obligations; retaining an immutable compiled source
does not establish parity for those effects. The original $shapesGraph helper's
unsupported behavior must not be presented as a supported original capability.

The typed compact cache representation is a parity foundation, with optimized
performance still unmeasured. Neither reader tests nor source relocation prove
full Graph/inference/rule publication parity or a speed improvement. Preserve
existing coverage denominators and require measured optimized artifacts before
making performance claims.
