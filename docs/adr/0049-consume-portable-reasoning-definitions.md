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
