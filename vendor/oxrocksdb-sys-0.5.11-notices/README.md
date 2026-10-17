# Bundled native notices

Unmodified license texts from the published `oxrocksdb-sys` 0.5.11 archive,
SHA-256 `01fcf5ae1e1295a612bef72fc64cd81e1ac3d1bd3243ef84d330f2962a8335e1`,
source revision `df37a5c98e2497135cdd4cfce01a049b78ca6740`.

The registry dependency builds RocksDB and LZ4 into the native Python extension.
RocksDB/oxrocksdb-sys use their Apache-2.0 option. The retained upstream files also
cover LevelDB-derived code (BSD-3-Clause) and LZ4's library (BSD-2-Clause). LZ4's
programs, tests and examples are not linked. This directory contains notices only,
not a fork of the registry dependency. Cargo.lock retains the source checksum.

The Python package license-files configuration includes these texts in source
archives and wheels. scripts/test_wheel.py verifies their presence and exact bytes
in each built wheel before running the installed-artifact tests.
