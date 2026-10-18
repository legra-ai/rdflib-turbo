# ADR 0018: Lease native dictionary text during projected reads

Status: Accepted

Projected reads previously decoded stored text into fresh Rust strings before
copying that text into the RDFLib objects required by the public API. Large
literal scans therefore allocated another payload-sized buffer for every row.
The fixed 128-row buffer reservation also wasted capacity when the byte budget
limited batches to far fewer rows.

Expose an owned lexical projection from the vendored store. In-memory dictionary
entries own shared immutable text; projected terms lease that text without
copying its payload. Inline encoded values remain inline. Persistent-store and
canonical scalar decoding retain their required owned text. The existing owned
term and projection APIs keep their contracts. Missing dictionary entries remain
storage errors, and exact lexical identity, datatype, language and RDF 1.2 term
forms remain explicit in the projected type.

Native bounded rows own lexical terms through Python materialization. Dictionary
guards and cursor locks are released before invoking Python factories. A lease
can outlive the cursor and store, and does not retain either. There is no term
cache, Python object mirror, unsafe borrowed pointer or mutable global state.

Start row capacity at 16, allocate only when a row is returned, and use the
previous bounded batch length as the next capacity hint. The 128-row and 64 KiB
payload limits, pending-row semantics and indivisible oversized terms are
unchanged. Capacity adaptation changes allocation only, never returned rows.

Regression tests verify shared payload addresses and lifetime after store drop,
exact metadata, persistent reopen, RDF 1.2 forms, released dictionary guards,
missing-entry errors, and cursor row and byte limits. An isolated allocation
regression scans 1,000 stored 4 KiB literals with less than 512 KiB of Rust heap
requests; the original implementation requested over 6 MiB. Installed-wheel
benchmarks determine the public runtime and memory effects separately.
