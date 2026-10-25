# Bundled native notices

Unmodified license texts from the published `oxrocksdb-sys` 0.5.13 archive,
SHA-256 `b3ab00e254d81066a73af8f26dcfd1acb76c9b4cc51d7d239e3ab37c3065fb4f`,
source revision `2837810c8993da671d496eabf7677d7573c5c522`.

The registry dependency builds RocksDB and LZ4 into the native Python extension.
RocksDB/oxrocksdb-sys use their Apache-2.0 option. The retained upstream files also
cover LevelDB-derived code (BSD-3-Clause) and LZ4's library (BSD-2-Clause). LZ4's
programs, tests and examples are not linked. This directory contains notices only,
not implementation sources. The runtime uses the matching patched source directory; rust-sources.json retains the original archive checksum.

The Python package license-files configuration includes these texts in source
archives and wheels. scripts/test_wheel.py verifies their presence and exact bytes
in each built wheel before running the installed-artifact tests.

The vendored source's local `lz4/.gitattributes` patch appends `* -text` after
upstream conversion rules. This preserves the checksum-verified published
archive bytes through both `git add` and checkout, including CRLF Windows
scripts and project files. Those files keep their original published hashes;
only the patched attributes file has an updated local manifest checksum.
`tests/test_source_bytes.py` verifies both staged blobs and checkout bytes with
the real nested attribute hierarchy under both autocrlf settings.
