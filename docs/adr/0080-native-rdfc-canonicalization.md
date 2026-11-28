# ADR 0080: Native RDFC-1.0 canonicalization and isomorphism

Status: Accepted; PH-21, alongside ADR 0079 slices 1 to 3

## Context

RDFLib offers `rdflib.compare.isomorphic`, `to_canonical_graph` and
`graph_diff` (RGDA1 colour refinement in Python) and an approximate
`Graph.isomorphic` that ignores blank-node structure and can report false
positives. It has no W3C RDF Dataset Canonicalization (RDFC-1.0).

The vendored oxrdf 0.3.5 contains an RDFC-1.0 implementation behind its
`rdfc-10` feature. It has no work limit: Hash N-Degree Quads explores every
permutation of each related blank-node group, and `generate_permutations`
materializes all of them, so a "poison" dataset (W3C case test074, a ten-node
blank-node clique) runs for factorial time and memory.

## Decision

1. Enable `oxrdf/rdfc-10`. Patch oxrdf (see its `TURBO-PATCH.md`) with
   `Dataset::canonicalize_blank_nodes_within(algorithm, CanonicalizationBudget)`,
   returning `CanonicalizationBudgetExceeded` once the budget is spent. One
   work unit is one Hash N-Degree Quads call or one permutation; a group's
   `n!` permutations are charged before they are generated, so exhaustion
   never allocates them. The upstream unlimited entry points are unchanged.
2. The native core (`src/canonical`) collects the selected quads from a store
   snapshot or a transaction session, labels blank nodes and renders sorted
   canonical N-Quads from the stored terms. Lexical forms are never
   normalized (ADRs 0006 and 0014). Generalized statements are rejected.
   Labelling runs after collection, outside the transaction session, so an
   exhausted budget fails the call without aborting the transaction; an
   invalid argument finishes the transaction, as `dump` does.
3. Two term identities:
   - `canonicalize` produces RDFC-1.0 output, which uses RDF 1.1 term
     identity: an explicit `xsd:string` literal is written as the simple
     literal with the same lexical form.
   - Isomorphism uses RDFLib term identity, where `"a"` and
     `"a"^^xsd:string` differ, so it agrees with `rdflib.compare.isomorphic`.
4. Python API:
   - `Graph.canonicalize(*, hash_algorithm=HashAlgorithm.SHA256,
     work_budget=DEFAULT_WORK_BUDGET) -> str` canonicalizes a graph's triples
     as a default-graph dataset; `Dataset.canonicalize` keeps graph names.
     `HashAlgorithm.SHA384` selects the spec's alternative hash.
   - `CanonicalizationBudgetExceeded` (a `RuntimeError`) reports exhaustion;
     the default budget is 1,000,000 units.
   - `Graph.isomorphic(other)` is exact, like `rdflib.compare.isomorphic`,
     rather than RDFLib's approximation.
   - `rdflib_turbo.compare` mirrors `rdflib.compare.__all__`: native
     `isomorphic`, `to_canonical_graph` (a `ReadOnlyGraphAggregate` over a
     native graph) and `graph_diff`; `IsomorphicGraph`, `to_isomorphic` and
     `similar` are RDFLib's own objects. RDFLib graphs passed in are copied
     explicitly with `Dataset.from_rdflib`. Datasets compare by quads,
     where RDFLib's functions fail on a Dataset's 4-tuples.

## Consequences

- The W3C rdf-canon suite is vendored at a pinned commit with SHA256SUMS and
  ratcheted in the `rdfc10` section of `tests/conformance-baseline.json`.
  RDFLib is graded where it has an equivalent: its `isomorphic` check that an
  evaluation case's input and canonical result are the same graph. Map,
  poison and named-graph cases have no RDFLib equivalent.
- Canonical blank nodes are `c14n` labels, not RDFLib's `cb` hash labels;
  `to_canonical_graph` stats contain only `triple_count`. Both are
  deterministic functions of the graph contents, as RDFLib documents.
- A canonicalization holds its selected scope in memory once, because
  RDFC-1.0 labels from the whole input and sorts the whole output.
- On poison inputs, isomorphism raises `CanonicalizationBudgetExceeded`
  instead of running unbounded; callers can raise `work_budget`.
