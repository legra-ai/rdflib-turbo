# ADR 0076: Restricted standard numeric inference on shared typed clauses

Status: Accepted for the explicitly authorized numeric production integration.

Add owl2-rl-numeric-core, selecting exactly four published rdf-reasoning 2.6.0
clauses: numeric/dt-eq, numeric/dt-diff, numeric/dt-type2 and numeric/dt-not-type.
Do not route the new selector through Python-reference initialization or rule
scheduling. Existing selectors keep their contracts. Optional reference axiom
flags are unsupported here and fail before publication.

Bind the sixteen XML Schema numeric datatypes through shared strict lexical
interpretation. Float/double spaces, signed zeros, subnormals, infinities and
canonical NaN identities follow OwlNumericValue. Keep keys as typed values in
native bindings; do not serialize them into RDF keys or conflate them with
Python cached numeric values. General OWL real/rational selectors, other literal
domains and nested RDF triple terms are outside this mode.

Interpret the shared checked clause AST through a deliberately restricted
adapter: positive atoms on generalized triples and numeric relations, plus
structural Ne. Validate operators/relations/arities before reading facts so an
empty relation cannot conceal unsupported syntax. Bind at most four variables
and one restartable source row at a time. Numeric literal occurrences come from
all three direct term positions; duplicate occurrences collapse at the existing
output frontier. Never collect the source literal domain or build a value-key
index in memory. Indexed triple patterns retain the backend's exact graph scope.

The four clauses introduce no literals. Their only type conclusions are proven
memberships and cannot introduce a new incompatible assertion. One immutable
pass therefore reaches their fixed point. Retain only missing generalized
consequences in the existing Pending frontier, then publish through the active
transaction/insertion journal. Time and distinct output can be quadratic in the
numeric domain; no bounded-memory materialization claim is made.

Join asserted rdf:type to explicit shared nonmembership for diagnostic witnesses.
Construct OwlNumericTypeViolation before formatting a stable identified message.
Publish diagnostics with existing namespace/outcome ownership after all rules
succeed. Invalid lexical forms and storage/read/insert failures abort the entire
native transaction, including preceding writes; they are execution errors rather
than guessed value classes or inconsistency messages.

This selects no ordinary RDFS/OWL clauses, equality replacement, lists,
cardinalities, dt-type1, nonnumeric domains, facets or other inconsistency checks.
Keep these exclusions public. Native Graph/Store/Dataset and transaction entry
points share the same mode path. Functional owlrl-turbo expansion must receive
its own explicit selector and a published supporting provider dependency minimum;
its compatibility Semantics classes keep their existing meanings.

Regression tests precede implementation. Native generalized consequences and
rollback, installed-wheel owner/scope/transaction tests and existing reference
compatibility tests are required. Do not publish companion metadata that accepts
an older runtime lacking this mode. Fresh immutable peer artifacts are required
for installed-wheel acceptance; historical artifacts cannot certify new source.
