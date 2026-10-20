# ADR 0034: Use indexed candidates for intersection membership

Status: Accepted

The intersection-membership CONSTRUCT rule joined every declared intersection
with every typed subject before applying correlated membership checks. Its
nested property paths also rescanned unrelated list entries. A connected seed
in the query text did not eliminate those rescans in the current evaluator.

Evaluate this rule with the reasoner's existing indexed graph interface. Follow
`rdf:rest*/rdf:first` once per intersection, deduplicating reachable nodes to
terminate cycles. Seed candidates from the type index for one required member,
then check the remaining members through exact indexed membership. Retain the
original universal typed-subject match when the reachable member set is empty.
This is property-path reachability, not strict list validation: dangling,
branching, cyclic and literal-valued structures preserve the original rule's
behavior. Other list rules keep their existing validation contracts.

Keep results in the existing pending-addition set. All rules still read the
same round snapshot; no new fact becomes visible before the round commits.
No query-planner fallback or second dataset is introduced. Traversal state
scales with one reachable list; pending results scale with derived output.

Regression tests compare exact candidates against the original CONSTRUCT rule
and use deterministic row counts on synthetic sparse inputs. The indexed path
must visit linearly many rows for that fixture; timing thresholds are excluded.
