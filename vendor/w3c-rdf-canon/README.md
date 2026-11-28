# W3C RDF Dataset Canonicalization (RDFC-1.0) test suite

Unmodified copies of every RDFC-1.0 test vector — evaluation, issued-identifier
map, SHA-384 and the negative "poison" case — graded by
`scripts/rdfc_conformance.py` and ratcheted by `tests/test_rdfc_conformance.py`
against the `rdfc10` section of `tests/conformance-baseline.json`.

- Source: <https://github.com/w3c/rdf-canon>
- Commit: `5ffaaa13c0e19a2a2f9e60d0ecd9efaf3edab633` (committed 2026-11-14, vendored 2026-11-28)
- License: dual W3C Test Suite License and W3C 3-clause BSD License; see
  `LICENSE.md` (repository root copy) and `tests/LICENCE.md` (suite copy).
- Integrity: `SHA256SUMS` covers every vendored file except this README.

Files keep their upstream paths: `tests/manifest.ttl` and every file of
`tests/rdfc10/` (`*-in.nq` inputs, `*-rdfc10.nq` canonical results and
`*-rdfc10map.json` issued-identifier maps). The generated manifest copies
(`manifest.jsonld`, `manifest.csv`), the HTML index and the Ruby generators
are omitted.

To refresh, copy the same file set from a new pinned commit, update this README
and regenerate the checksums from this directory:

```sh
find . -type f ! -name README.md ! -name SHA256SUMS | LC_ALL=C sort | sed 's|^\./||' | xargs shasum -a 256 > SHA256SUMS
```
