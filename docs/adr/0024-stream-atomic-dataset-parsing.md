# ADR 0024: Stream Dataset parsing through the insertion journal

Status: Accepted

NativeDataset.load previously decoded the entire input into owned Quad values
before inserting it. Repeated lexical values occupied memory once per input row
even though the final Dataset interns them once. Atomic input does not require
retaining this second decoded relation.

Use the existing additive Dataset insertion transaction from ADR 0005. Parse and
insert one quad at a time under the Dataset write lock. Preserve parser base IRI
and blank-node scoping options. Collect namespace bindings after successful parsing
and before committing the insertion journal, then bind them under the same lock.
A parser error drops the journal, removing newly inserted statements and newly
interned terms. Existing statements and namespace bindings remain unchanged.
Readers cannot observe partial input; concurrent readers now wait for parsing as
well as insertion. No second Dataset, parsing pass or fallback is introduced.

Run the parser peak-allocation regression alone. Its prebuilt input contains
many distinct subjects sharing a large literal: the final interner retains one
lexical value, so the regression distinguishes full decoded staging from streaming
without confusing input size with required retained data. A separate late-error
regression checks native allocation release, exact data and namespace rollback,
and successful reuse. Both join the exact-inventory CI allocation-budget runner.

This removes a parser allocation defect. It does not promise to eliminate the
whole-process memory difference in the three-product pipeline. The legacy Dataset
still owns six indexes, inference retains its additions and transaction journal,
and queries, validation results, interpreter imports and allocator arenas have
separate costs. Store cardinality metadata does not change this Dataset owner.
