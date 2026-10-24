# Generalized dataset query reads

`&GeneralizedDataset` implements the existing `QueryableDataset` interface with
borrowed `TermCow` values. All statement positions retain their full `Term`
domain, including literal subjects and blank or literal predicates. Pattern
references may be temporary; returned rows borrow the stored dictionary. Query
constants remain query-owned terms and are not inserted into the dataset.

Graph names keep their existing named/blank/default domain and typed identity.
The query interface distinguishes default-graph matching, one named graph and all
named graphs. Named graph enumeration streams existing GSPO graph prefixes, with
no strict-quad narrowing, graph-name set or additional index. This in-memory owner
does not represent empty graph declarations.

`GraphRead` also accepts the generalized owner. It emits full generalized quads
at its existing owned-row boundary and streams named identities from the same
index. It does not copy the underlying dataset. Fallible query sources retain
their errors; an error is not an empty result.

The ordinary `Dataset` adapter and default generic CONSTRUCT output policy remain
unchanged. SELECT/ASK can read generalized term roles; generic CONSTRUCT retains
its strict RDF subject/predicate projection and existing constant/generated-blank
internalization contract (ADR0028). Custom callback term coercion still follows
the evaluator's existing default or explicit exact-term feature policy.

Focused regressions cover graph scope, all term masks, raw blank-node identities,
long borrowed lexical values, temporary patterns, typed callbacks, source errors,
strict CONSTRUCT and optional quoted/directional RDF1.2 terms. These adapter tests
do not by themselves establish complete public Graph/inference parity, native
execution coverage, or a performance improvement.
