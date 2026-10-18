# ADR 0030: Reuse identifiers within bounded read batches

Status: Accepted

Native graph reads reconstructed identical RDFLib identifiers for every result.
The dictionary already leases immutable shared lexical text, so repeated terms
can be recognized without hashing strings or retaining another graph.

Materialization may reuse identifiers within one batch. Each SPO position owns
four entries holding a native lexical owner, identifier kind and Python object.
Compare shared string identity and length; retain its owner throughout the entry
lifetime to prevent address reuse. URI and blank-node kinds remain distinct.
Identifiers above 256 UTF-8 bytes bypass reuse. Eight consecutive eligible misses
disable that position for the remaining batch. Literals are never reused.

Activate reuse only for batches with at least eight rows, to amortize setup.
The existing reader still limits batches to 128 rows and approximately 64 KiB.
At most twelve extra identifiers are retained, and all entries drop before the
batch returns. No dictionary, cursor or graph gains a Python-object cache.

The standard immutable identifier factories opt in explicitly. Custom factories
remain uncached by default; callers opting in accept fewer identifier callbacks.
Python allocation, decrefs, errors and reentry occur after native scan locks have
been released. Errors preserve the original exception and close continuations.
Local stack references add no persistent Python GC edges.

Regressions cover standard reads, batch boundaries, short batches, UTF-8 limits,
literal callback counts, same-text URI/blank-node separation, miss disabling,
reentrant cursor close and failures after partial output and cache population.
Performance validation must retain unique-value and selective-read controls;
bounded memory alone does not prove that reuse improves every workload.
