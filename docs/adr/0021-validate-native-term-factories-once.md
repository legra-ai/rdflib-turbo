# ADR 0021: Validate native term factories once

Status: Accepted

Every scoped read previously extracted four Python factory handles and repeated
two string-subclass and two callable checks. The standard adapter supplies the
same immutable tuple for every read, so this repeated boundary validation does
not represent new input or changing native storage state.

Replace the private tuple argument with a frozen NativeTermFactories Python
class. Its constructor accepts the identifier classes, literal restorer and
string allocator, validates their capability once, and owns those four handles.
The standard adapter creates one immutable descriptor. Custom callers explicitly
construct their own descriptor; read_pattern accepts only that descriptor type.
Remove the displaced tuple route rather than retaining a fallback.

First-batch materialization borrows the frozen descriptor after native scanning
has released its locks and reacquired the GIL. A continuation owns one Python
reference to the descriptor. Its GC traversal visits that one edge; the
descriptor traverses its four actual owned references. Do not share a Rust Arc of
four Python handles across cursors, because repeating traversal from each cursor
would misrepresent the actual Python reference counts.

Callbacks still run once for every requested output term. No Python term cache,
mutable Rust global state, graph mirror or alternate storage path is introduced.
Callback errors retain their original exception and release the native cursor;
reentry and reference cycles follow the existing materialization contract.
Runtime callback failures remain errors even after successful construction.

Regressions reject the displaced tuple, validate constructor arguments, check
immutable reuse, callback errors/reentry, and verify a descriptor shared by two
cyclic cursors remains alive with an external owner and becomes collectable when
that owner disappears. Installed-wheel paired measurements determine performance
separately; removing repeated checks alone does not establish a runtime win.
