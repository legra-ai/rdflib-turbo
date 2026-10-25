# ADR 0067: Keep native quad version locks inline

Status: Accepted for the native memory-store experiment; installed measurement pending.

The Graph inference path uses the native MVCC Store, not the compact standalone
Dataset. Actual macOS profiling of a 60,348-quad inferred graph found one retained
quad record per statement and approximately one separately allocated 80-byte
pthread mutex per record. The immutable original artifact and full output
identity remain recorded outside the repository. This identifies a storage cost,
not a whole-input clone or duplicate returned Dataset.

Use `parking_lot::Mutex` only for each quad's `VersionRange`. Its uncontended lock
is inline and does not allocate a per-quad OS mutex on first access. Keep the
existing standard mutex/condition-variable writer coordination, publication
locks, version transitions, indexes, dictionary, journal and persisted bytes.
Do not introduce custom unsafe synchronization or replace the production
allocator. The dependency's existing lock_api/parking_lot_core dependencies are
already in the graph; preserve unrelated locked package versions and edges.

The private version critical sections only read or change `VersionRange`; they
execute no user callbacks. The inline lock does not provide standard mutex
poisoning. It must not be used to recover from a failed operation or repair state:
normal transaction error/rollback behavior and fail-fast invariant checks remain
unchanged. Future fallible or user-provided work must stay outside these guards.

Require the actual quad node's first-touch allocation regression, as well as
existing concurrent publication, rollback, retained snapshot, exact-term and
generalized relation tests. Test-only thread-local allocation counters forward
unchanged requests to System; they are not production allocator changes.

A focused unit regression establishes removal of backing allocations, not wheel
RSS savings or complete parity. Rebuild the real source archive/wheel and compare
the same complete-output workload before making a memory/performance claim.
Fingerprint the changed native manifest/source through the existing mechanism.
