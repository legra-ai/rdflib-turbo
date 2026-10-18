# ADR 0013: Consolidate RocksDB thread status metadata definitions

Status: Accepted

A persistence-enabled native extension executes RocksDB C++ loader initializers
before Python's module initialization. The original 0.5.11 build contains 143
initializer entries, whose entry addresses span 105 distinct 16 KiB code pages.
These are binary address spans, not measured RSS savings. Deferring PyO3 class
registration cannot prevent those constructors from running.

RocksDB's thread_operation.h defines five static metadata tables containing
std::string values. Transitive header inclusion creates independent tables and
startup constructors in many translation units, including otherwise unused
thread-status metadata in compaction code. Only thread_status_impl.cc reads the
tables. This is redundant initialization, rather than a base-dataset copy.

Vendor the checksum-verified published oxrocksdb-sys 0.5.11 archive, preserving its
revision and license notices. Declare the existing tables in the header and define
them once in thread_status_impl.cc under the same feature guard. Keep their
contents, public thread-status APIs, persistence capabilities and storage contract.
Do not disable RocksDB, introduce another extension, or share Rust pointers.

A regression compiles the actual header in two translation units and checks that
all five table addresses refer to one definition. It fails before the change and
passes afterward. Native persistence tests and actual installed-wheel startup,
RSS and workload benchmarks verify integration and quantify any improvement.
