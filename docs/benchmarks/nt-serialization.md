# Reproduce N-Triples serialization measurements

The [recorded results](nt-2026-10-23.json) compare RDFLib 7.6.0, the published
rdflib-turbo 0.2.0a14 wheel and unreleased candidate source
`c1966915fb9c50a902eab729bb7bfd694c513490`. The candidate was built with
`uv build` from its original independently audited source distribution, using
CPython 3.13.16 and Maturin's locked release profile. The documentation commit
containing this report is not the measured native build source.

This snapshot includes the URI correction at
`faa0b268cd5433f18134568a64ae7dbc24d1268c`: it preserves RDFLib's invalid-URI
exceptions without aborting a caught pending transaction. All 99 NT contracts and
169 neighboring checks passed against the actual optimized wheel before timing.
The [initial measurement](nt-2026-10-23-initial.json), which predates that correction,
is retained as historical evidence and does not describe the corrected code.

The fixture cycles through plain strings, explicit `xsd:string`, Unicode French
language strings, noncanonical integer spellings and IRI objects. Three controls
exercise blank-node IDs, escaping and datatype identity; another named graph must
remain excluded. `--rows` excludes the three controls: use `99997` for exactly
100,000 triples. Output byte spelling/order need not match; RDF lexical terms,
datatype presence, language and blank-node IDs must match.

Create independent native uv projects, outside this checkout. Use the exact
candidate wheel identified in the JSON when available, or build the recorded
source on your platform; a new build will have its own archive and wheel hashes.
The candidate has not been published as the current PyPI package.

```sh
uv init --bare --no-workspace --vcs none --python 3.13.16 /tmp/nt-python
uv add --project /tmp/nt-python rdflib==7.6.0
uv init --bare --no-workspace --vcs none --python 3.13.16 /tmp/nt-published
uv add --project /tmp/nt-published rdflib==7.6.0 rdflib-turbo==0.2.0a14
uv init --bare --no-workspace --vcs none --python 3.13.16 /tmp/nt-candidate
uv add --project /tmp/nt-candidate rdflib==7.6.0 /path/to/candidate.whl

# Run from this repository. Each invocation is a new process.
uv run --project /tmp/nt-python --locked --no-sync python -I scripts/benchmark_nt.py --arm python --rows 99997 --scope default --encoding str
uv run --project /tmp/nt-published --locked --no-sync python -I scripts/benchmark_nt.py --arm prior --rows 99997 --scope default --encoding str
uv run --project /tmp/nt-candidate --locked --no-sync python -I scripts/benchmark_nt.py --arm candidate --rows 99997 --scope default --encoding str
```

Use `--encoding utf8` for bytes. Repeat each case six times, rotating all six arm
orders. Small controls use `--rows 10` with each of `default`, `named`, `blank`
and both encodings; the intermediate case uses `--rows 1000`. The script requires
the recorded interpreter/reference version and rejects optional PyOxigraph.
It checks the serializer implementation actually selected, so an old native
wheel cannot silently count as the candidate fast path.

For a new candidate source build, first freeze its checkout, then build and audit
the source distribution before building that exact archive:

```sh
uv build --sdist --out-dir /tmp/nt-source /path/to/frozen-checkout
uv run --no-project python scripts/audit_sdist.py /tmp/nt-source/rdflib_turbo-0.2.0a15.tar.gz
uv build /tmp/nt-source/rdflib_turbo-0.2.0a15.tar.gz --wheel --python 3.13.16 --out-dir /tmp/nt-wheels -C 'maturin.build-args=--locked --profile release'
```

Keep original archives, hashes, locked dependency files and installed package
hashes. The recorded candidate's engine fingerprint was independently derived
from the actual immutable Git engine source and six patched dependencies; it is
not a fingerprint of provider serialization code. Archive-to-Git and installed
wheel payload comparisons separately verify that code.

Recorded runs were serial and bounded by a 90-second/1,280-MiB whole-child-group
watchdog, including setup and the later correctness audit. All 90 children passed.
CPU and wall timers cover only `Graph.serialize`; peak RSS is captured before
correctness parsing and includes imports, graph construction and output. It is a
process high-water mark, not isolated serializer heap. Complete exact multisets
are compared through 1,003 triples; larger cases retain count and three
order-independent SHA-256 accumulators, which are probabilistic witnesses.
Shared-host load and measured CPU variance are included in the result metadata.
