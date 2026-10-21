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
