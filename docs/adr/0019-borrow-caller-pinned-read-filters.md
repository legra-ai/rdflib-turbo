# ADR 0019: Borrow caller-pinned text for committed read filters

Status: Accepted

Extracting read-pattern tuples into Rust Strings copied caller-owned Python text
before encoding native filter keys. A membership lookup with a pre-existing 4 MiB
Python literal requested over 4 MiB of Rust heap allocations even though no RDF
term was returned.

Extract committed read filters into PyO3 PyBackedStr owners. Keep those owners on
the attached caller frame and borrow their text during the detached native call.
A shared borrowed view validates term kinds, literal metadata, statement positions
and language syntax. Prepared native references borrow lexical text; only a
language tag requiring lowercase normalization allocates replacement text.

Filter encoding finishes synchronously before the prepared references or caller
pins are dropped. Returned cursors own their native snapshots and lexical output
leases, never Python filter objects. Python factories still run after native locks
are released and the GIL is reacquired. Extraction never calls Python __str__.

Owned mutation inputs continue moving their Strings into native model objects.
They use the same validator at the existing transaction owner boundary, preserving
abort behavior after invalid operations. No Python-backed request is sent to the
transaction worker, and no new detached Python-reference destruction path is added.

A test through the actual Python method trampoline reproduces the original copy
and now requires less than 512 KiB of Rust allocations for the same 4 MiB filter.
Installed-wheel tests cover temporary filter collection, snapshot independence,
Unicode and surrogate behavior, graph scope, exact datatype/language identity and
invalid metadata. Rust tests compare owned and borrowed validation and verify
borrowed lexical pointer identity. Allocation budgets run alone in CI processes;
Cargo's successful zero-test result is rejected by inventory validation.
