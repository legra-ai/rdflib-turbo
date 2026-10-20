# ADR 0033: Use 128-bit SipHash-1-3 for exact lexical keys

Status: Accepted

Fully bound membership hashes caller-pinned lexical text into native storage
keys on each call. Large lexical values make the hash rounds a substantial
part of lookup work, even after transport tuples and lexical copies have
been removed. Caching Python inputs would add another ownership and lifetime
contract rather than reducing the cost of an uncached lookup.

With `exact-term-storage`, use the existing dependency's 128-bit SipHash-1-3
implementation for lexical keys. Keep the fixed initial keys, 16-byte key
width and big-endian representation. Every encoding boundary uses this same
choice, including insertion, query filters, update, transaction and bulk load.
The ordinary upstream feature configuration retains SipHash-2-4 and its fixed
hash vectors. See the [siphasher API](https://docs.rs/siphasher/latest/siphasher/sip128/index.html).

Persistent exact-term stores must declare `turbo-strhash = siphash13-128` in
their existing default metadata column family. Opening a store validates that
field before any queries or storage migration can run. A missing declaration
is initialized only when every column family is empty. Missing or mismatched
declarations on existing stores are hard errors for both readers and writers;
there is no inference, retry with another hash or conversion path. A new
declaration is flushed together with the existing initial storage metadata.
The key layout and existing storage version field are retained. Builds without
`exact-term-storage` reject any `turbo-strhash` declaration before migration,
including unknown schemes or incomplete version metadata.

There is no additional RDF index, retained input cache, Python owner or
mutable global state. Native literal spelling, explicit datatypes, language
handling and RDF term distinctions remain governed by ADR 0006. Memory and
persistent stores share the same key scheme.

Regressions pin ASCII and Unicode hash vectors and require them in the native
CI test inventory. Storage tests cover declared, missing and mismatched
schemes, read-only reopen and unchanged metadata after rejected opens. The
existing feature-off, RDF 1.2, exact query/update and persistence contracts
remain required. Installed-wheel measurements must retain unrelated graph
operations, the original Python comparison and memory controls.
