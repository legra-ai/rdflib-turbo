# ADR 0073: Bind the original literal value key directly

Status: Accepted

The one-time RDFS substitution clause first reads an assertion's object, then
joins it with a separately scanned distinct literal/value-key relation. The
object is already bound by the assertion. This creates a redundant graph scan,
distinct set and hash join before the necessary replacement lookup.

Validate the complete published typed clause and its ordered roles before
lowering this dedicated capability. Require a triple assertion, original and
replacement value-class atoms, matching head subject/predicate and replacement,
matching value keys, and five distinct variable roles. Reject unexpected shapes.
Bind the existing custom value-key function directly to the assertion's actual
object and retain its BOUND guard inside a projected subquery, without DISTINCT.
The guard must run before the join: a join could otherwise fill an undefined
original key from the replacement row and admit an invalid original object.
The replacement relation still scans the same
immutable source and deduplicates exact literal/key pairs. Do not change generic
rule compilation or the published reasoning definition.

Retain numeric preflight, caller lexical context, generalized head transport,
private pending additions, graph scopes and transaction publication. Undefined
value keys still eliminate the original row. No lexical normalization, Python
evaluator fallback, stored auxiliary relation or graph copy is introduced.

A regression requires the actual prepared plan to retain its value-keyed
replacement join while removing the original-object join. Malformed role tests
fail before execution. Existing exact string/numeric substitutions, Decimal
signals, generalized source/destination fixtures and read/insert failure tests
remain required. Allocation and installed-wheel measurements determine whether
this implementation improves the pipeline; query-plan counts alone are not a
performance claim.
