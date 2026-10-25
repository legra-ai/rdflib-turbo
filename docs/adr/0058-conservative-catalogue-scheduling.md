# ADR0058: Skip flat catalogue clauses with unchanged input predicates

Status: Accepted

The fixed-point scheduler reevaluated every catalogue query every round, even
when none of its positive body relations changed. Keep prepared query metadata
derived from the same validated typed rule definitions. A constant body predicate
selects its catalogue slot; variable predicates depend on every publication.
Virtual list membership depends on both actual rdf:first and rdf:rest relations.
Exact inequality filters introduce no additional graph relation.

Run every clause in the bootstrap round. While publishing a successful complete
round, accumulate only a fixed-width mask over catalogue predicate slots. On the
next immutable round, skip constant-predicate clauses whose inputs did not change.
The fixed catalogue must fit64 slots; reject an expanded unsupported inventory
explicitly. Never retain input graph predicates, an extra delta graph, a base
copy, or a second output relation. Keep descriptor storage bounded by the fixed
catalogue, independent of graph size.

Count every published predicate conservatively, including destination-only
publications. This can execute extra clauses for separately scoped destinations
but cannot hide a source-visible change. Equality, local cardinality clauses,
intersections, chains, keys and diagnostics retain their existing per-round
execution and wider membership visibility. Preserve immutable round publication,
the original clause order and outer transaction rollback. Do not eagerly close
equality components or expose another clause's pending output.

This is conservative scheduling, not semi-naive evaluation: active clauses still
join their entire visible relation. Type-heavy and wildcard-heavy workloads may
skip little work. A synthetic RDFS regression compares actual query calls with
the former full-inventory behavior. Additional tests compare every flat pending
set against unscheduled full queries, including late list edges, new schema and
variable predicate inputs. Existing native scope/rollback/closure tests remain
required. Performance and memory acceptance require a separately attested
optimized installed wheel; passing query-count tests is not a speed claim.
