# ADR 0027: Return exact Graph membership as a native boolean

Status: Accepted

Fully bound committed Graph membership already uses the exact native quad index,
but its result previously passed through a row vector, placeholder triple, term
factories, a Python batch and generators. None of these values is needed to answer
whether the quad exists.

Add a typed NativeStore.contains_terms operation returning bool. Reuse the same
term preparation and exact lookup as bound row reads. Pin caller-owned strings
during detached validation and lookup; never retain Python references in storage.
Iteration still returns its existing rows because those callers request triples.

Select the boolean path only for fully bound ordinary committed Graph patterns.
Preserve class, instance and custom attribute-dispatch triples overrides before
inspecting the pattern. Partial patterns, property paths and transaction-backed
Graphs retain their established semantic implementations. Dataset triple/quad and
union membership remain separate. Do not catch native errors or retry another
implementation.

Installed-artifact regressions first reject row transport during exact presence
and absence checks, then cover overrides, graph scopes, exact literal identity,
closed handles, invalid inputs and transaction behavior. A separate isolated
allocation budget exercises the actual Python binding with a caller-owned large
lexical value and verifies boolean output without allocating a result row. It joins
the required exact-inventory CI budget runner. Installed-wheel benchmarks determine
whether removing this transport overhead improves membership performance.
