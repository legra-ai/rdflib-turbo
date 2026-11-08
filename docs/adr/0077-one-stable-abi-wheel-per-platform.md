# ADR 0077: Build one stable-ABI wheel per platform

Status: Accepted; cross-platform installed release validation remains mandatory

Supersedes ADR0020 for newly prepared releases. Preserve every existing release's
original archive inventory and bytes; recovery still checks out that release's
prepared revision and its original packaging rules. Preserve the workflow-owned
wheel-policy controller before that checkout, then derive the matrix from the
prepared manifest. Its explicitly audited historical CPython mode retains the
original seven cohorts/25 wheels; new abi3 sources select five jobs/wheels.
Unknown feature/range combinations fail. Only abi3 sources run the stable-ABI
audit, since old prepared packaging-tool locks do not declare that tool.

Build the sole native provider with PyO3's `abi3-py311` feature through `uv build`.
Each of the five existing platforms produces one `cp311-abi3` wheel. Test that
identical original wheel, including every installed native/Python file hash, on
ordinary CPython 3.11, 3.12, 3.13, 3.14 and 3.15. Do not replace the 25 installed
acceptance combinations with five interpreter tests. Retain all native coverage,
persistence, source-archive, companion and aggregate checks. Audit every wheel
with the independently locked `abi3audit --strict` before acceptance/publication.
The aggregate requires exactly five platform wheels and one source archive.

The previous scalar-only `_PyUnicode_*` imports are not part of Python's stable
ABI. Replace those imports with compact read-only classification and lowercase
tables generated from each supported interpreter's own Unicode behavior. Select
the exact table with `Py_Version`, immutable stable-ABI data since Python 3.11.
Validate the host Unicode database version during module initialization. There
is no Rust-Unicode fallback, per-character Python callback, mutable global cache
or full scalar-domain allocation retained at runtime. Two-level deduplicated
256-codepoint blocks provide classification/decimal lookup; sparse sorted pairs
provide simple lowercase. ASCII takes its exact invariant fast path.

`scripts/generate_host_unicode.py` reproduces/checks every codepoint using public
`str` and `unicodedata` operations and checks simple lowercase against the host
regex scalar oracle. The checked-in provenance records interpreter and Unicode
versions, counts, lengths and SHA-256 hashes. Source tests verify the complete
inventory and reproduce the current host's tables; installed tests exercise the
same compiled policy on every supported interpreter. Generated data ships in the
binding crate's existing `src/**` source inclusion.

ADR0063's host name-capsule lease, finite audited interpreter range, original
exception identity and explicit interpreter-owned regex authority remain. The
capsule is obtained and validated at runtime through stable object/capsule APIs;
it is not a direct private-symbol import. Stable-ABI packaging does not authorize
an unaudited capsule layout or a different host regex authority. Package metadata
therefore declares Python >=3.11,<3.16. Supporting another Python/Unicode version
requires auditing and testing it before extending that range.

An isolated release-profile prototype passed strict stable-ABI audit with zero
violations and ran the same wheel on all five interpreters: 312 SDK tests and
3,428 SHACL tests per interpreter, with the existing expected xfail. Five paired
fresh-process application samples per case had identical complete observations;
median differences ranged from approximately -5% to +3%, with overlapping samples.
These are packaging-change experiments, not a fresh original-versus-turbo product
benchmark or proof that every operation is faster. Full platform CI remains the
release gate. Symbol stripping is confined to the release profile; development
coverage retains its symbols. Build and publication remain native uv operations
through the single existing tested `ci.yml` publisher.
