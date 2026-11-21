# ADR 0079: Move the turbo internals onto the shared model crates

Status: Proposed; design only, no implementation is authorized by this record

## Context

rdflib-turbo, owlrl-turbo and pyshacl-turbo are drop-in replacements for
RDFLib, OWL-RL and pySHACL that must be much faster. Those two properties are
the acceptance criteria for every change in this record. They are not traded
against tidiness, crate reuse or a smaller fork.

All three packages run one native extension, built from this repository
(ADR 0068, ADR 0077). Its Rust today rests on a semantic fork of the Oxigraph
family in `vendor/`: oxrdf 0.3.5, oxigraph 0.5.13, spargebra 0.4.8, sparopt
0.3.9, spareval 0.2.9 and oxsdatatypes 0.2.4, about 61,000 lines with 526
lines of patch notes. The fork supplies exact-term storage (ADR 0006, 0033),
additive insertion journals (ADR 0005, 0035), pinned MVCC read views (ADR
0037), projected and lexical cursors (ADR 0014, 0018), generalized statements
(ADR 0065) and exact SPARQL builtins. The zero-copy shared-dataset design of
ADRs 0032 to 0045, 0050 and 0052 borrows those views directly.

Separately published, independently versioned crates now model the same
domains: rdf-structural-model (RDF 1.2 terms), datalog-structural-model
(rule programs), rdf-reasoning (RDF/RDFS/OWL rule definitions and presets),
inference-engine (a Datalog executor over caller-supplied storage),
shacl-structural-model (SHACL 1.2 and AF declarations), owl-structural-model
(OWL 2 structural values) and sparql-results (SRJ/SRX streaming). They are
model crates. None of them parses RDF documents, stores quads or evaluates
SPARQL, by explicit scope decision in each crate.

Current adoption is partial and pinned behind the published versions:

| Crate | Pinned here | Current | Used for |
| --- | --- | --- | --- |
| rdf-structural-model | not a dependency | 0.6.0 | nothing |
| datalog-structural-model | =3.0.0 | 3.0.5 | shared clause lowering (ADR 0049) |
| rdf-reasoning | =2.6.4 | 2.17.0 | RDFS table and common OWL property clauses (ADR 0049, 0053, 0073) |
| inference-engine | not a dependency | 0.1.0-alpha.2 | nothing; executes only `rdfs-subclass` and `rdfs-core` |
| shacl-structural-model | =6.4.4 (engine) | 6.4.7 | shape declarations (pyshacl-turbo ADR 0017) |
| owl-structural-model | not a dependency | current | nothing |
| sparql-results | not a dependency | 0.3.0 | nothing; results are RDFLib `Result` objects |

The model crates do not agree on one rdf-structural-model release:
shacl-structural-model pins `=0.3.9` and inference-engine pins `=0.4.0`.
Two releases of the same crate are distinct Rust types, so terms cannot cross
between them without conversion.

The SHACL engine in `crates/engine` reads shapes into shacl-structural-model
(`model.rs` aliases its generic types over `oxrdf::Term`), then lowers them
into its own parallel execution structures in `compile.rs`, `constraints.rs`
and `components.rs`. It also carries a second, older reasoner:
`crates/engine/src/inference.rs` holds hand-written SPARQL CONSTRUCT tables
for RDFS and OWL 2 RL, reached only through the binding library's standalone
`infer` and `infer_into`. The provider's own reasoner in `src/inference/` is
the one owlrl-turbo and shared validation use; it already consumes
rdf-reasoning for its shared clauses and keeps tuned kernels for equality
(ADR 0036), intersections (ADR 0034), lists, keys, datatypes and diagnostics.

Conformance evidence is in place and is the gate for this record:

- `tests/conformance-baseline.json`: RDF 1.1 Turtle 313/313, TriG 357/357,
  N-Triples 70/70 and N-Quads 87/87 (RDFLib 7.6.0: 713 of 827); RDF 1.2
  syntax and evaluation suites with 142 ledgered failures, none worse than
  RDFLib.
- owlrl-turbo `tests/owlrl-goldens`: 24 fixtures frozen at OWL-RL 7.6.2;
  `tests/divergences.toml` holds one intentional entry (`haskey-all-properties`),
  one gap (`language-tag-case`) and two converged entries.
- pyshacl-turbo: the corrected pySHACL suite at 403/403, SHACL 1.0 and 1.2
  W3C grading and the direct contract, reconciled exactly against
  `tests/divergences.toml` (31 entries, 8 gaps; pyshacl-turbo ADR 0047).

The published speed results are the performance gate. They are recorded in
the README of each package:

| Benchmark | Reference | Turbo | Source |
| --- | ---: | ---: | --- |
| Parse, RDFS, SHACL, 10,000 nodes | 15.889 s | 0.444 s (35.8×), 126.7 MiB | shared-dataset-macos-2026-11-03 |
| Parse, OWL-RL, SHACL, 10,000 nodes | 11.104 s | 0.940 s (11.8×), 123.4 MiB | same |
| Matched Graph, literal reflexivity | 285.2 ms | 25.5 ms (11.2×) | matched-graph-macos-2026-10-31 |
| Matched Graph, literal typing | 362.7 ms | 31.5 ms (11.5×) | same |
| N-Triples, 100,000 triples, `str` | 484.21 ms | 110.14 ms (4.4×) | docs/benchmarks/nt-2026-10-23 |
| SHACL rules, 1,000 nodes | 1,247.07 ms | 40.81 ms (30.6×) | pyshacl-turbo benchmarks/drop-in |
| SPARQL patterns, 1,000 nodes | 6,023.02 ms | 47.98 ms (125.5×) | same |
| SHACL with RDFS, 1,000 nodes | 340.37 ms | 155.19 ms (2.19×) | pyshacl-turbo benchmarks/reasoning-current |
| SHACL with OWL-RL, 1,000 nodes | 622.26 ms | 163.00 ms (3.82×) | same |

The small-input SHACL cases are already slower than pySHACL (for example
`shapes-1000-nodes-10-shapes`, 0.08×). Moving internals must not make them
worse; closing that gap is a separate deliverable (below).

## Decision

Move ownership of each *model* onto the shared crates, keep ownership of
*storage, parsing and SPARQL evaluation* behind one adapter over the existing
Oxigraph fork, and move the *rule definitions* to rdf-reasoning while the
provider keeps its tuned execution kernels until a shared executor matches
them. Each move is a separately shippable slice with the gates below.

### Component ownership

| Component | Today | Owner after migration | Stays in the turbo repositories |
| --- | --- | --- | --- |
| Terms and literals | `oxrdf::Term` with the exact-term patches; language tags lowercased on construction | rdf-structural-model `Term`, `Literal`, `LanguageTag` at every engine, reasoning and report boundary | The `TermBridge` between storage encodings and model terms; the RDFLib term mapping in Python |
| Dataset and store | Patched oxrdf `Dataset` and oxigraph `Store`, journals, MVCC views | Not a shared crate. A `turbo-store` adapter module in this repository, initially over the Oxigraph fork | All of it: storage, transactions, journals, borrowed views |
| Parsers and serializers | oxttl, oxrdfxml, oxjsonld through oxrdfio, plus the native N-Triples writer | Not a shared crate | All of it; output is produced from model terms through the adapter |
| SPARQL evaluation | Patched spargebra, sparopt, spareval with exact builtins and cached plans | Not a shared crate | All of it, behind the same adapter |
| SPARQL result formats | RDFLib Python `Result.serialize` | sparql-results, only if a measured native `serialize` wins (open question 4) | The RDFLib `Result` facade |
| RDFS and OWL-RL definitions | rdf-reasoning for RDFS and common property clauses; hand-written programs for the rest; a duplicate CONSTRUCT reasoner in `crates/engine` | rdf-reasoning presets and datalog-structural-model programs for every rule, including equality, lists, keys, datatypes and diagnostics | owlrl Python compatibility semantics (axiom options, `hasKey` correction, diagnostics namespaces) as ledgered profile choices |
| RDFS and OWL-RL execution | Provider fixed-point scheduler, prepared CONSTRUCT lowering, tuned kernels | Provider scheduler first; inference-engine per profile once it meets the budgets | The kernels registered as accelerated implementations of named rules |
| SHACL shape model | shacl-structural-model read, then parallel engine structures | shacl-structural-model types used directly by the compiler; engine keeps only compiled indexes, regexes and prepared queries | The compiler and pySHACL boolean/input quirks |
| SHACL validation | `crates/engine` over `GraphRead` and `QueryGraph` | `crates/engine` (no shared validator exists) | All of it |
| SHACL-AF and 1.2 rules | Engine executes TripleRule and SPARQLRule with pySHACL iteration order | AF rules: engine, unchanged semantics. SHACL 1.2 inference rules: lowered to datalog-structural-model and executed by the same reasoner as RDFS | pySHACL rule ordering and iteration limits |
| Reports | Engine result structures, RDFLib report graph and text | Engine; report terms become model terms | pySHACL report text byte for byte |
| OWL structural values | Not used | owl-structural-model only where an API exchanges OWL axioms (none today; ADR 0049) | Nothing |
| Python boundary | PyO3 bindings in `src/` and `crates/python` | Unchanged owner | All of it; the bridge to RDFLib terms |

### Vendored Oxigraph

The model crates cannot replace the fork, because they do not store or query.
Something must. The candidates are weighed against the two acceptance criteria:

1. **Keep the Oxigraph fork behind an adapter (chosen).** Define
   `turbo-store` traits in this repository for the operations the engine and
   reasoner actually use: exact quad scans by pattern and graph scope,
   membership, journaled staging, pinned snapshots and prepared SPARQL over a
   view. Implement them once over the fork. All existing zero-copy paths
   (ADR 0019, 0032, 0037, 0050, 0052) keep borrowing the same views, so no
   copy is introduced. This changes no behaviour and costs nothing at run time
   if the traits are static (generic, not `dyn`) on hot paths.
2. **Return to unpatched upstream Oxigraph.** Rejected as long as upstream
   canonicalizes typed literals in storage and lowercases language tags:
   RDFLib's lexical preservation cannot be recovered after storage discards
   it (ADR 0006). Patches that upstream accepts are dropped from the fork one
   at a time; that is fork reduction, not a migration step.
3. **A new store and SPARQL evaluator written for these crates.** Rejected
   for now. It would rebuild about 40,000 lines of query engine to arrive
   where the fork already is, and every benchmark above would have to be
   re-earned. It becomes a candidate only if the adapter shows the fork
   blocking a requirement that patches cannot meet.
4. **Another existing Rust store.** No candidate preserves exact lexical
   forms, generalized statements, RDF 1.2 terms and journaled transactions
   together. Rejected on the same evidence basis as option 2.

What leaves the fork, and when:

- `oxrdf::Term` leaves every engine, reasoner and report signature in slices
  2 to 6. It remains the storage-side encoding inside `turbo-store`.
- The duplicate CONSTRUCT reasoner in `crates/engine` leaves in slice 3.
- Language-tag lowercasing leaves the term-construction path in slice 4.
  Storage identity for mixed-case tags follows the reference test in Risks.
- Parsers, storage and SPARQL stay in the fork for the whole migration. Their
  exit is not scheduled; it requires a new record with its own evidence.

### Rule definitions versus execution

Every rule's *definition* moves to rdf-reasoning, so owlrl-turbo, pySHACL
`inference=` and SHACL 1.2 rules share one catalogue. *Execution* stays a
backend choice. A tuned kernel, such as the equality row groups of ADR 0036,
registers as the implementation of named rule IDs. A differential test runs
the generic lowering of the same definitions against the kernel on the
goldens and on generated inputs, and the two must produce the same closure.
No rule exists only as a kernel.

inference-engine replaces the provider scheduler for a profile only when it
meets that profile's benchmark budget on the shared-dataset and matched
Graph fixtures. Its executor is tuple-at-a-time over async cursors; the
provider evaluates set-at-a-time with hash joins. Until a profile passes, the
inference-engine path runs only in a test-only differential mode.

## Slice plan

Every slice is one or more pull requests, each releasable on its own. Every
slice has all of these gates, in addition to its own:

- **G-RDF**: `tests/conformance-baseline.json` unchanged or better; no new
  known failure.
- **G-OWL**: owlrl-turbo goldens unchanged; `tests/divergences.toml` exact
  reconciliation green, with no new entry.
- **G-SHACL**: pySHACL upstream 403/403; SHACL 1.0 and 1.2 W3C grading and
  the direct contract reconcile exactly with pyshacl-turbo's ledger; no new
  entry.
- **G-PERF**: paired A/B against current main on the same host, fresh
  processes, the existing harnesses. Candidate median within 5% of main and
  peak RSS within 5%, for every row of the table above. A slice may not lower
  any published speedup by more than 5% relative to its recorded value.
- **G-WHEEL**: each abi3 wheel within 10% of 0.2.0a41's size (11.0 MB macOS
  arm64, 12.2 MB Linux arm64, 13.5 MB Linux x86-64, 14.2 MB Windows); CI
  wall time within 15% of current main.

| # | Slice | Repositories | Own gates | Size |
| --- | --- | --- | --- | --- |
| 1 | Align model versions: rdflib-turbo on rdf-reasoning 2.17, datalog-structural-model 3.0.5, shacl-structural-model 6.4.7; shacl-structural-model and inference-engine released on one rdf-structural-model release | rdflib-turbo, shacl-structural-model, inference-engine | One rdf-structural-model version in `Cargo.lock` | S |
| 2 | `turbo-store` adapter traits and `TermBridge` (fork encoding to rdf-structural-model, owned and borrowed); no caller switched | rdflib-turbo | Lossless round-trip property test over all RDF 1.1 and 1.2 term kinds, generalized positions and exact literals; zero new allocations on borrowed reads (allocation tests) | M |
| 3 | Remove the duplicate reasoner: route `crates/python` `infer` and `infer_into` to `src/inference`; delete `crates/engine/src/inference.rs` | rdflib-turbo | pyshacl-turbo `inference=` RDFS and OWL-RL workloads within G-PERF | M |
| 4 | Language-tag spelling: inferred heads keep the input spelling through model terms; case-insensitive equality stays | rdflib-turbo, owlrl-turbo | owlrl `language-tag-case` moves from gap to converged; RDFLib `Literal` equality tests unchanged | S |
| 5 | SHACL compiler on shacl-structural-model types directly; parallel engine structures removed; report terms as model terms | rdflib-turbo, pyshacl-turbo | `shapes-*` compile rows within G-PERF; report text byte-identical on every direct-contract case | L |
| 6 | Rule catalogue complete in rdf-reasoning (equality, lists, cardinality, keys, datatypes, diagnostics), one family per pull request; kernels registered against rule IDs with differential tests | rdf-reasoning, rdflib-turbo | Differential test green per family; the OWL-RL rows within G-PERF | L |
| 7 | inference-engine `ReasoningStore` over `turbo-store` views, run differentially; switched on per profile only when it meets G-PERF | inference-engine, rdflib-turbo | Closure equality with the provider scheduler on all goldens; no profile switched below its budget | L |
| 8 | SHACL 1.2 inference rules lowered to datalog-structural-model and executed by the slice 6/7 reasoner; AF rules keep pySHACL semantics | shacl-structural-model, rdflib-turbo, pyshacl-turbo | SHACL 1.2 rule cases move from not-run to graded; no pySHACL-parity case changes | M |
| 9 | Fork reduction: offer exact-term storage and string identity patches upstream; drop each patch that upstream accepts | rdflib-turbo | `vendor/` patch notes shrink; G-RDF and G-PERF | S each |

Sizes: S is up to about 500 changed lines, M up to about 2,000, L more and
split by rule family or component.

## Relation to the other planned deliverables

- **RDF 1.2 (turning on `rdf-12`)**: after slice 2, before slice 5. Triple
  terms, reifiers and directional literals map once in `TermBridge` onto
  rdf-structural-model, which is RDF 1.2 native, so the Python mapping and the
  nesting-depth guard are written once. The 142 ledgered RDF 1.2 failures are
  that deliverable's ratchet.
- **RDFC-1.0 and native isomorphism**: independent, may run alongside slices 1
  to 3. Canonicalization reads a dataset; it belongs in `turbo-store` over
  oxrdf's `rdfc-10` feature. No model crate canonicalizes. RDF 1.2 triple
  terms are outside RDFC-1.0 and are refused explicitly.
- **Loss ledger for lossy export**: after RDF 1.2 and slice 2. A loss is
  defined against the rdf-structural-model domain (triple terms, direction,
  graph, lexical form), so it needs the bridge first.
- **pyshacl-turbo total result order and canonical report gate**: after
  RDFC-1.0, together with slice 5, since slice 5 rewrites the report term
  path.
- **pyshacl-turbo typed ValidationLimits**: after slice 3 (one reasoner left
  to limit) and before slice 7, whose `RunLimits` it maps onto. Defaults
  reproduce pySHACL exactly.
- **pyshacl-turbo small-input fixed cost**: before slice 5. Its cache keys on
  compiled shapes and its measurements become slice 5's baseline. It does not
  depend on the migration.

## Risks

- **Zero-copy borrowed views.** Model terms are owned values by default; a
  naive bridge would copy every lexical form. Mitigation: `TermBridge` yields
  rdf-structural-model's borrowed values over the fork's leased `Arc<str>`
  text (ADR 0018). Slice 2's allocation tests must show zero added
  allocations per borrowed row before any caller switches.
- **Datalog versus tuned CONSTRUCT speed.** A generic tuple-at-a-time
  executor can lose an order of magnitude on equality and lists, where ADR 0036
  avoids a cubic intermediate. Mitigation: definitions move, kernels stay
  (slice 6); inference-engine switches per profile only on a passing budget
  (slice 7); neither is required for drop-in behaviour.
- **Literal lexical preservation.** Any conversion through a value type, or
  through upstream oxrdf constructors, can normalize `"01"^^xsd:integer` or a
  decimal spelling. Mitigation: the bridge maps lexical forms only, never
  values; G-RDF and the goldens include exact-spelling cases; numeric-reference
  fixtures in `src/inference` run on every slice.
- **pySHACL quirks reproduced on purpose.** pyshacl-turbo ADR 0007 (document
  versus Graph boolean decoding, closed shapes ignoring `rdf:type
  rdfs:Resource`), ADR 0021 (standard RDF inference output) and ADR 0078 here
  (unknown vocabulary ignored) are deliberate. A cleaner shared model can
  remove them silently. Mitigation: quirks live in the compiler, not in the
  model crates; every intentional ledger entry must still reconcile.
- **Language-tag case.** Moving to case-insensitive model tags closes the
  owlrl gap but can split a term into two spellings in storage, or merge two
  terms RDFLib keeps apart. Mitigation: before slice 4, a reference test
  records how RDFLib 7.6.0 and OWL-RL 7.6.2 treat `"a"@EN` and `"a"@en` in
  one graph; storage identity follows that result and the test stays.
- **Wheel size and build time.** Every new crate adds code to one cdylib, and
  inference-engine adds async machinery. Mitigation: G-WHEEL; inference-engine
  enters behind a Cargo feature that published wheels enable only with slice
  7's switch; thin LTO and stripped symbols remain.
- **Version skew between model crates.** Mitigation: slice 1, then a CI check
  that `Cargo.lock` holds one version of each model crate.
- **Async executor in a synchronous extension.** inference-engine's traits
  return futures. Mitigation: drive them with a single-threaded local executor
  on the detached native thread; no tokio runtime and no GIL is held.

## Open questions for the owner

1. **Storage and SPARQL.** (a) Keep the Oxigraph fork behind `turbo-store`
   indefinitely; (b) also plan a store of our own now; (c) keep it and
   revisit only when the fork blocks a requirement. Recommendation: (c).
2. **Kernels versus shared executor.** (a) Require inference-engine for all
   execution; (b) definitions in rdf-reasoning, tuned kernels allowed with
   differential tests; (c) leave rules where they are. Recommendation: (b).
3. **SHACL-AF rules.** (a) Lower AF TripleRule and SPARQLRule into Datalog
   too; (b) keep pySHACL's rule semantics in the engine and lower only SHACL
   1.2 inference rules. Recommendation: (b); pySHACL order and iteration are
   not a fixed point.
4. **SPARQL result formats.** (a) Add native `Result.serialize` through
   sparql-results; (b) leave results in RDFLib. Recommendation: (a) only if a
   paired benchmark shows a gain, after a synchronous writer exists, since
   sparql-results is tokio-based today.
5. **owl-structural-model.** (a) Adopt it now for OWL-RL; (b) adopt it only
   where an API exchanges OWL axioms. Recommendation: (b), as ADR 0049 says.
6. **Performance budget.** (a) 5% median and RSS per slice as above; (b) no
   regression at all; (c) a looser 10%. Recommendation: (a), with any loss
   repaid before the next slice.
7. **Order against RDF 1.2.** (a) Turn RDF 1.2 on before the bridge; (b)
   after slice 2. Recommendation: (b).

## Consequences

The model crates become the one place where terms, rules and shapes are
defined, and the turbo packages stop maintaining copies of them. Storage,
parsing and query stay on the Oxigraph fork behind an adapter, so the fork is
still ours to carry. Each slice is reversible on its own and is merged only
with every gate green. owlrl-turbo ADR 0013 and pyshacl-turbo ADR 0048 point
here. This record changes no code and claims no speedup or conformance.
