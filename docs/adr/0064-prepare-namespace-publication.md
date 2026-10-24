# ADR 0064: Prepare and reserve namespace publication before RDF commit

Status: Accepted for source wiring; coordinated installed-wheel acceptance pending.

Graph transactions, Dataset transactions and public inference deltas previously
committed native RDF before copying namespace maps into the caller. A copy failure
therefore raised after RDF had already changed. A competing bind during detached
native acceptance could also be overwritten by the later namespace replacement.

Prepare both independent namespace maps and the next revision before committing
RDF. Reserve the caller's existing NamespaceBindings object after checking its
expected revision. Hold the same nonblocking per-mapping lock through native
acceptance and publication. Publish only preallocated references and revision;
release the reservation on native failure. Never restore a copied RDF base or
share mutable maps with an escaped transaction facade.

Bind normalizes caller arguments before acquiring this guard, then changes maps
and revision under it. Two-map snapshots use the same guard. Busy mutation,
snapshot or publication attempts raise explicitly; they never block or retry.
Ordinary namespace lookups and iteration acquire no additional lock.

For transaction facades, metadata preparation and reservation occur inside the
native context before its exit commits RDF. Public infer_delta finishes its
requested result and metadata preparation before accepting pending inference.
Tests inject failures in either map, observe competing binds at the actual native
commit boundary, preserve graph domains and namespace-owner identity, and retain
independence from escaped transaction metadata.
