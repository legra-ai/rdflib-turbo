# W3C RDF syntax test suites

Unmodified copies of the W3C RDF 1.1 and RDF 1.2 Turtle, TriG, N-Triples and
N-Quads test suites, graded by `scripts/w3c_conformance.py` and ratcheted by
`tests/test_w3c_conformance.py` against `tests/conformance-baseline.json`.

- Source: <https://github.com/w3c/rdf-tests>
- Commit: `7b5afb91857a9c282c03d2e45d3f76485797c901` (committed 2026-11-12, vendored 2026-11-17)
- License: dual W3C Test Suite License and W3C 3-clause BSD License; see
  `LICENSE.md` (repository root copy) and the per-suite `LICENSE` files.
- Integrity: `SHA256SUMS` covers every vendored file except this README.

Directories keep their upstream paths under `rdf/`:

| Directory | Manifest |
| --- | --- |
| `rdf/rdf11/rdf-turtle`, `rdf-trig`, `rdf-n-triples`, `rdf-n-quads` | RDF 1.1 suites |
| `rdf/rdf12/rdf-turtle/{syntax,eval}`, `rdf/rdf12/rdf-trig/{syntax,eval}` | RDF 1.2 suites |
| `rdf/rdf12/rdf-n-triples/syntax`, `rdf/rdf12/rdf-n-quads/syntax` | RDF 1.2 suites |

Only manifests and test documents (`.ttl`, `.trig`, `.nt`, `.nq`) are copied.
Upstream implementation reports, HTML/HAML templates, archives and the RDF 1.2
`c14n` serializer suites are omitted; the RDF 1.2 parent manifests that only
`mf:include` those directories are omitted too. RDF 1.2 cases are expected to fail
until RDF 1.2 parsing is enabled; each such failure is ledgered with its reason.

To refresh, copy the same file set from a new pinned commit, update this README
and regenerate the checksums from this directory:

```sh
find . -type f ! -name README.md ! -name SHA256SUMS | LC_ALL=C sort | sed 's|^\./||' | xargs shasum -a 256 > SHA256SUMS
```
