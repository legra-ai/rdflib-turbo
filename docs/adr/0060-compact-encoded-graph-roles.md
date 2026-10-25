# ADR 0060: Store only graph-name variants in encoded quad graph positions

Status: Accepted for the private native-storage prototype; installed measurement pending.

An encoded quad previously retained four full encoded terms. The graph position
permits only default graph, named node and blank node, while the full term also
contains literal and optional quoted-triple variants. Actual native size tests
show that retaining this broader domain unnecessarily enlarges each quad.

Introduce a separate encoded graph-name enum with exactly the existing default,
named, numeric blank, inline blank and hashed blank representations. Preserve
all 128-bit identifiers and hashes, the complete inline-string capacity and the
existing hash policy. Conversion from a general encoded term is checked and
rejects literal or quoted-triple graph positions. Ordinary generalized subject,
predicate and object domains remain unchanged.

Only the quad's graph field changes its in-memory type. Existing graph-name
indexes, dictionaries, transaction logs, scope filtering and decoder interfaces
retain their established term types, using allocation-free value conversions at
the quad boundary. Do not add a side table, graph copy, new index or language
spelling payload. Preserve every existing persisted key byte and storage version.

Tests must exercise the actual native quad layout, every graph representation
and all nine index byte orders, invalid literal graph slots, default/named/blank
scope identity, pending transactions, rollback, empty graph declarations, owned
snapshots and persistent reopen. Both exact and upstream storage profiles require
these cases in the native test inventory. The separately demonstrated maximum
prefix carry defect must remain fixed and tested independently.

On the measured 64-bit native build, the graph enum occupies 17 bytes and the
quad decreases from 160 to 144 bytes. This is a requested-layout reduction, not
an installed RSS result: allocator size classes can charge the same allocation
size for both node layouts. Actual source-package, wheel identity and matched
whole-output measurements are required before claiming a runtime memory win.
