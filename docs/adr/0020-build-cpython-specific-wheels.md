# ADR 0020: Build and test CPython-specific wheels

Status: Accepted

Build the Python extension against the CPython API for each supported interpreter.
Remove PyO3's `abi3-py311` feature; retain its safe public APIs, the extension-module
build feature and the existing persistence feature. There is one implementation
and no ABI3 fallback. Source package metadata continues to require Python 3.11+.

The release matrix covers ordinary CPython 3.11, 3.12, 3.13, 3.14 and 3.15 on the
existing five native platforms. macOS minimums remain in the Maturin target tables.
The installed PyO3 version supports CPython through 3.15 without a forward
compatibility flag. Free-threaded, debug and development interpreter ABIs are
outside this matrix.

Keep one job per platform. Extract the verified source archive once, then build
all five interpreter wheels from that stable directory with `uv build --python`.
Reuse the same Cargo target and vendor paths. Manylinux builds run all five
interpreter builds inside one container, retaining dependency and compiler caches
without mounting the parent checkout (ADR 0003).

Each wheel must run the full Python suite with 100% line and branch coverage in a
separate uv project pinned to its matching interpreter. Before testing, verify the
actual interpreter, package location and every installed Python/native file hash
against the supplied wheel. Every wheel retains all bundled native licenses.
Native coverage and companion integration keep their existing measured-source and
runtime fingerprint checks.

Release inventory is exactly one source archive and the Cartesian product of five
CPython ABIs and five platforms: 26 archives. Reject missing, additional, ABI3 or
mismatched artifacts. Publication still restores and publishes original tested
archives with exact hashes through `ci.yml`; it does not rebuild them. An existing
published release bundle cannot be replaced with a different wheel inventory.
