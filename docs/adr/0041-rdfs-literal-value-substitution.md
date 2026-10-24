# ADR 0041: Join RDFS literals by their reference value profile

Status: Accepted; string-valued profile implemented, other profiles pending

The original RDFS one-time rule substitutes literal objects whose Python values
compare equal. RDF term identity and SPARQL value equality implement different
contracts. In particular, equal string values substitute across language tags,
plain and explicit strings, and the default string-valued datatype converters.
Unknown datatypes do not gain string semantics merely from matching lexical text.

Implement this rule once before RDFS cyclic reasoning. OWL-RL and combined modes
have their own initialization and must not inherit this RDFS-only substitution.
Retain the original exact RDF terms as replacements, including their datatype
and language. Read the caller's existing native graph scope and transaction.

Project each side's value key before the join, allowing the native evaluator's
hash join to select equal keys. Do not reintroduce the reference's Cartesian
product over all literals, or call a Python reasoner. The retained pending set
contains only new output facts. Native query intermediates and the inference
delta retain ADR 0008's existing memory limits; this is not a bounded-memory
query guarantee.

Require bound keys explicitly and teach the optimizer that a positive BOUND
filter removes undefined bindings. Preserve the existing term-kind restrictions
and do not infer this fact through OR or NOT. A prepared-plan regression requires
the real keyed join; execution regressions retain SPARQL's unbound wildcard
behavior whenever the filters allow it.

The initial string profile follows values reconstructed from the native store's
exact lexical terms. Programmatically constructed RDFLib literals can retain
cached Python values from before lexical normalization; preserving that state
is an unresolved transport contract, not an accepted parity exception. Numeric,
binary, temporal, XML and context-sensitive value profiles require their own
reference comparisons. This decision does not claim complete RDFS value parity.

## Numeric reference value classes

Use rdf-reasoning 0.4.0's LiteralRuleId and ExactNumber for the string and numeric
value-class join. The runtime supplies the relation explicitly; ordinary shared
rule compilation rejects it without that capability. Keep exact source literal
identities in the result and require bound keys on both sides of the hash join.

Default RDFLib numeric conversion is separate from AltXSD datatype diagnostics:
booleans compare with numbers, integer subtypes use Python integer values without
AltXSD range checks, and both float and double use binary64. Decimal coefficients
and binary floats share exact finite keys without rounding decimal 0.1 into
binary 0.1. Signed mathematical zeros join; positive and negative infinities
have separate keys. NaNs never acquire an equality key. The reference identity
publication remains separate, retaining each original statement even for NaN.

Capture integer digit limits and Decimal status in the existing invocation-owned
context. Propagate signaling-NaN comparison exceptions before publishing the
substitution delta, retain Decimal conversion flags, and record FloatOperation
when the input includes comparable Decimal and float categories. A scalar pass
records these effects without constructing a Cartesian literal relation.

The fixed 26-term oracle checks 1,352 reference pairs, including subnormal floats,
large exponents, invalid lexicals and both InvalidOperation trap settings. This
extends the existing RDFS reference profile; it does not define standard OWL
numeric datatype identity or resolve cached/custom Python literal values,
binary/temporal/XML equality, or complete Python owlrl compatibility.
