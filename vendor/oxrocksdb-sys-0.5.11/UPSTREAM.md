# Vendored oxrocksdb-sys 0.5.11

Source: the published `oxrocksdb-sys` 0.5.11 crate archive from crates.io.
Archive SHA-256: `01fcf5ae1e1295a612bef72fc64cd81e1ac3d1bd3243ef84d330f2962a8335e1`.
The archive was verified against the repository's original Cargo.lock before extraction.
Upstream Git revision: `df37a5c98e2497135cdd4cfce01a049b78ca6740`, path `oxrocksdb-sys`,
as recorded in the unmodified `.cargo_vcs_info.json`.

The crate declares GPL-2.0-only OR Apache-2.0. RocksDB's original dual-license files remain
at `rocksdb/COPYING` and `rocksdb/LICENSE.Apache`, with the LevelDB notice at
`rocksdb/LICENSE.leveldb`. LZ4's original licensing remains at `lz4/LICENSE` and
`lz4/lib/LICENSE`. All notices supplied in the published archive are preserved.

## Local patch

`rocksdb/util/thread_operation.h` declares the five existing thread status/name
metadata tables with extern linkage. Their definitions moved without changing
contents into `rocksdb/monitoring/thread_status_impl.cc`, inside the original
`NROCKSDB_THREAD_STATUS` feature guard. Header inclusion no longer constructs
independent std::string tables in every translation unit. Thread-status APIs,
RocksDB persistence and supported storage formats remain intact.

`patch-tests/thread_table_identity.py` compiles the real header in two translation
units, links the real implementation, and checks that all five table addresses
are identical. It fails against the original published static definitions and
passes after consolidation. Run from the project root:

```sh
uv run --no-project python vendor/oxrocksdb-sys-0.5.11/patch-tests/thread_table_identity.py
```

The local `.cargo-checksum.json` records the verified source archive checksum and
current vendored file checksums, including the documented patch and regression.
Static loader-page counts motivate the change; installed-wheel measurements are
required before claiming an RSS or startup improvement.
