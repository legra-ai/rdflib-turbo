# Inference memory measurements

Whole-process measurements for the three companion turbo projects live in the
[versioned pipeline benchmark](https://github.com/legra-ai/pyshacl-turbo/blob/main/benchmarks/releases.md).
Those installed-wheel runs compare original Python, historical turbo, latest
published turbo and exact candidate wheels, with full observations and cold RSS.

To distinguish transient native heap from Python/runtime/allocator overhead, run:

```sh
uv run --no-project --python 3.13 python scripts/profile_memory.py \
  --data /absolute/path/data.ttl --shapes /absolute/path/shapes.ttl \
  --inference owlrl --output /tmp/native-memory/profile.json
```

The script builds a separate release **test binary** with the optional
`allocation-profiling` feature, selects the matching embedded Python runtime and
runs its ignored profile test alone. It does not modify an installed extension.
The JSON and adjacent raw log retain input hashes, exact revision, tracked-tree
dirty status, interpreter/platform and allocation counts for parse, inference,
shape compilation, native validation/report serialization and release.

The allocator records requested live bytes, peak live bytes, cumulative requested
bytes and allocation calls. A realloc records the replacement's requested size;
allocator-internal relocation/arena overhead is outside these counters. Python's
own allocations and the facade's RDFLib report parsing are also outside them.
Use the full pipeline RSS results for process memory, and uninstrumented wheels
for performance. Profiling counters are compiled only into this opt-in test
binary; normal wheels have no allocation counter.

CI also runs `allocation_tests::inference_heap_budget` alone with one test thread.
Its fixed 1,000-record inference workload requires peak requested Rust heap to
stay below twice retained heap. This targets transient duplication, rather than
noisy OS RSS. The original clone-and-buffer implementation fails the budget.
Transaction regressions and full pipeline output comparisons independently guard
failure atomicity and reasoning semantics; a smaller heap is not permission to
discard inferred facts or reports.

The optimization keeps one dataset for default-graph inference, journals only new
interned keys, and retains only distinct new rule results per round. Named-graph
reasoning still needs one union view. The six dataset indexes, query evaluation,
SHACL evaluation and report creation still allocate memory. Lower transient
inference memory does not establish lower total memory than original Python.
