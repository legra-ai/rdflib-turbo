# ADR 0002: Test native failure boundaries and correct the shared datatype parser

Status: Accepted

Rust unit tests exercise the native runtime directly, including lock poisoning,
malformed RDF lists, inference validation, exact-key reasoning and graph context
preservation. Wheel builds enable PyO3 extension mode through maturin; standalone
Rust tests link the selected Python interpreter. The coverage runner supplies that
interpreter's base prefix so embedded Python loads the matching standard library.

A coverage regression exposed oxsdatatypes 0.2.3 rejecting February 29 as an
xsd:gMonthDay. Vendor that exact crate and correct its day validator to use the
1972 leap-year reference already used by its value representation. Keep the parser
shared by native consumers. Preserve original source checksum/revision and license
texts, and run upstream/parser regressions in required CI. The companion
pyshacl-turbo uses the same correction (its ADR 0033).

Coverage scope and counters remain unchanged. Missing invariant/error regions
stay visible; tests do not mark unexecuted code as covered.
