# ADR 0049: Execute portable reasoning definitions through a native adapter

Status: Accepted for the initial RDFS and common OWL property clauses

Use published rdf-reasoning definitions and datalog-structural-model rules as
the source of truth for shared clauses. Lower their positive, three-place
triple atoms into backend-owned prepared CONSTRUCT queries. Preserve checked
variable and IRI identities; reject other relations, arities, operators and
term forms before execution. Do not interpret unsupported operations as empty
rules or weaker semantics.

The existing fixed-point scheduler, round publication, graph scopes and native
transaction ownership remain backend responsibilities. The reference RDFS
initialization still seeds resources separately. Standard RDF output projection
remains the existing runtime contract; this adapter does not yet implement the
generalized intermediate term domain required by the full shared core presets.
Consequently it must not expose those preset names as fully supported modes.

Initial adoption replaces the local RDFS table and the domain, range,
subproperty, subclass, symmetric, transitive and inverse property clauses shared
with OWL. Remaining OWL rules, datatype operations, equality, lists, diagnostics
and Python compatibility semantics still need extraction. Neither adding shared
dependencies nor passing this slice establishes full OWL-RL or owlrl parity.

OWL structural ontology trees are not a prerequisite for RDF-based reasoning.
Reuse structural types where the API actually exchanges those values, rather
than converting whole graphs or adding unused dependencies to imply adoption.

Tests execute the real shared domain clause against backend terms and reject
unsupported or invalid rule forms. Existing native and installed-wheel suites
continue to guard the externally observable closure and transaction behavior.

## Extended relational adoption

The runtime now consumes rdf-reasoning 0.2.0 for functional properties,
value restrictions, equivalent-class membership and flat schema rules. The
versioned Owlrl762RuleId clauses preserve the pinned reference's guarded
schema consequences separately from standard RuleId definitions. Lower their
BuiltinFilter::Ne to !sameTerm, never SPARQL value inequality. Other operators
still fail before execution.

The indexed equality kernels and list/cardinality execution remain native.
The retained local equality queries guard their existing one-round performance
contract. This extension changes the source of rule definitions, not transaction
ownership or the standard RDF output boundary. Generalized terms, datatype
semantics, lists, diagnostics, extension rules and custom Python hooks remain
separate compatibility work; full-profile names remain unsupported as presets.

## Positive list adoption

With rdf-reasoning 0.3.0, use ListRuleId for cls-int2, cls-uni, cls-oo, scm-int
and scm-uni. The adapter implements the declared binary list-member body relation
through native rdf:rest*/rdf:first traversal. This supplies exactly the two
auxiliary clauses' reachable-member relation without storing membership facts
in the caller's graph. Reject auxiliary heads and wrong arities explicitly.

Cycles, branches and dangling structures have the same reachability semantics
as the displaced local property paths. This does not validate list structure or
change the separate indexed intersection-recognition kernel. The five local
list templates are removed. Cardinality and the equality kernel boundary remain
native; this still does not certify full OWL-RL or Python owlrl compatibility.

## Elide equivalent-class self-membership witnesses

The catalogue-specific CaxEqc1/CaxEqc2 factory adds an exact-term inequality
between the membership body's class and the head's class. When those terms
are identical, the head is already the same membership fact read by the body.
`Pending::push` rejects that existing fact; a scoped graph checks both its read
source and destination, so this remains true for a separate output graph.
Filtering that witness avoids constructing and probing redundant rows without
changing any newly publishable consequence or fixed-point round visibility.

Derive the guard from the typed published head and two body atoms. Check their
relations, arities, RDF predicates, shared focus variable and directional class
positions; reject an unexpected catalogue shape. Preserve generic `try_new`
semantics for arbitrary clauses. Apply this optimization only to the two known
catalogue IDs, using `BuiltinFilter::Ne` lowered to `!sameTerm`, never value `!=`.
Non-self equivalence and distinct numeric lexical terms continue to infer both
directions. Default, named and blank scopes, union reads and separate output
graph tests retain the existing source/destination contract. This does not
remove a rule, change its shared definition or establish universal OWL parity.

## Exact catalogue body groups

The fixed portable catalogue contains repeated identical bodies with distinct
heads. Validate every original rule's safety, head, body, terms and supported
operators before grouping. Combine heads only when their complete ordered body
ASTs are equal, including variable names, filter order and inequality operand
order. Retain the first body's catalogue position and append each original
validated head in order. Generic single-rule compilation keeps its existing
contract. Do not normalize expressions or infer equivalence from rendered text.

This reduces prepared portable plans from 12 to 11 in RDFS, 49 to 43 in OWL-RL
and 56 to 49 in combined mode. The five local OWL plans remain separate; combined
mode therefore evaluates 54 plans rather than 61. These are fixed plan counts,
not a timing improvement claim. Grouping retains only the bounded published
catalogue, never caller graph data, and adds no storage/index/dependency changes.

All grouped consequences still enter the original pending set. Every ordinary,
equality, list, chain and key rule reads the same immutable round before any
publication. Frozen ungrouped catalogue tests compare complete one-round output,
including exact literals, blank nodes, union/selected contexts and malformed
scoped-list errors. Late query failures leave pending results private and roll
back the existing insertion journal. Installed-wheel matched performance and
whole-output acceptance remain required before claiming a runtime speedup.

Catalogue-specific equivalent-class self-membership guards are applied before
grouping. The conservative predicate scheduler retains one descriptor for each
common body; identical ordered bodies necessarily have identical positive
predicate dependencies. Late class seeds and equivalent-class facts compare
every scheduled pending set with independent ungrouped catalogue queries.
