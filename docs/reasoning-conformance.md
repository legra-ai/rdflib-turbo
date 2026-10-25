# Reasoning ownership and conformance inventory

Audit baseline: rdflib-turbo `cce7d80` (2026-10-25), after #85–#91.
The consumer pins rdf-reasoning **1.0.0** and Datalog model **2.0.0**.
Portable main at `86b6be9` is **2.0.0** with Datalog model **3.0.0**;
that newer contract is not silently adopted here.

This is an ownership and gap inventory, **not a conformance certificate**.
A shared definition, a local implementation, or Python parity evidence does not
prove the complete standard rule over its required term/value domain. No complete
`owl2-rl` preset is enabled by this document. Runtime `owlrl`/`both` modes target
versioned Python behavior and are distinct from portable standard presets.

Normative identifier inventory: OWL 2 Profiles, 2012 Recommendation, tables 4–9:
https://www.w3.org/TR/2012/REC-owl2-profiles-20121211/#Reasoning_in_OWL_2_RL_and_RDF_Graphs
The table lists every one of its 78 identifiers exactly once. Multi-head rules
can map to several single-head portable clauses. The offline inventory guard
checks omissions/duplicates; it does not test semantic completeness.

## OWL-RL rules

`RuleId` and `ListRuleId` name portable definitions, not independently rewritten
backend rules. Local entries identify execution/compatibility ownership that
still needs extraction or standard-domain evidence. Diagnostic messages stand
in for the consumer's inconsistency policy, not a portable Datalog false head.

| Standard rule | Portable ownership | Current consumer binding / limitation |
| --- | --- | --- |
| `cax-adc` | Not a portable clause | [local binding](../src/inference/diagnostics/lists.rs); full-suffix class diagnostics; standard coverage unverified |
| `cax-dw` | Not a portable clause | [local binding](../src/inference/diagnostics/relations.rs); disjoint-class messages; standard coverage unverified |
| `cax-eqc1` | `RuleId::CaxEqc1` | [shared adapter](../src/inference/shared.rs); checked redundant-self guard in adapter |
| `cax-eqc2` | `RuleId::CaxEqc2` | [shared adapter](../src/inference/shared.rs); checked redundant-self guard in adapter |
| `cax-sco` | `RuleId::Rdfs9` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `cls-avf` | `RuleId::ClsAvf` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `cls-com` | Not a portable clause | [local binding](../src/inference/diagnostics/relations.rs); reference complement message; standard coverage unverified |
| `cls-hv1` | `RuleId::ClsHv1` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `cls-hv2` | `RuleId::ClsHv2` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `cls-int1` | Not a portable clause | [local binding](../src/inference/intersections.rs); indexed reachable-member recognition; standard coverage unverified |
| `cls-int2` | `ListRuleId::ClsInt2` | [shared adapter](../src/inference/shared.rs); auxiliary membership supplied by native paths |
| `cls-maxc1` | Not a portable clause | [local binding](../src/inference/diagnostics/restrictions.rs); caller Python zero-value profile; standard coverage unverified |
| `cls-maxc2` | Not a portable clause | [local binding](../src/inference.rs); local cardinality-one query; standard coverage unverified |
| `cls-maxqc1` | Not a portable clause | [local binding](../src/inference/diagnostics/restrictions.rs); qualified zero-value profile; standard coverage unverified |
| `cls-maxqc2` | Not a portable clause | [local binding](../src/inference/diagnostics/restrictions.rs); qualified Thing zero branch; standard coverage unverified |
| `cls-maxqc3` | Not a portable clause | [local binding](../src/inference.rs); qualified cardinality-one query; standard coverage unverified |
| `cls-maxqc4` | Not a portable clause | [local binding](../src/inference.rs); qualified Thing cardinality-one query; standard coverage unverified |
| `cls-nothing1` | Not a portable clause | [local binding](../src/inference/axioms.rs); fixed class axiom; standard coverage unverified |
| `cls-nothing2` | Not a portable clause | [local binding](../src/inference/diagnostics/relations.rs); Nothing-instance messages; standard coverage unverified |
| `cls-oo` | `ListRuleId::ClsOo` | [shared adapter](../src/inference/shared.rs); auxiliary membership supplied by native paths |
| `cls-svf1` | `RuleId::ClsSvf1` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `cls-svf2` | `RuleId::ClsSvf2` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `cls-thing` | Not a portable clause | [local binding](../src/inference/axioms.rs); fixed class axiom; standard coverage unverified |
| `cls-uni` | `ListRuleId::ClsUni` | [shared adapter](../src/inference/shared.rs); auxiliary membership supplied by native paths |
| `dt-diff` | Missing standard datatype operation | Not enabled as standard value reasoning; Python reference deliberately skips it |
| `dt-eq` | Missing standard datatype operation | Not enabled as standard value reasoning; Python reference deliberately skips it |
| `dt-not-type` | Not a portable clause | [local binding](../src/inference/diagnostic_datatypes.rs); Alt lexical checks, not full value membership; standard coverage unverified |
| `dt-type1` | Not a portable clause | [local binding](../src/inference/axioms.rs); fixed recognized datatype inventory; standard coverage unverified |
| `dt-type2` | Not a portable clause | [local binding](../src/inference/axioms.rs); own-type/ancestor seeding only; standard coverage unverified |
| `eq-diff1` | Not a portable clause | [local binding](../src/inference/diagnostics/relations.rs); same/different messages; standard coverage unverified |
| `eq-diff2` | Not a portable clause | [local binding](../src/inference/diagnostics/lists.rs); reference excludes final member; standard coverage unverified |
| `eq-diff3` | Not a portable clause | [local binding](../src/inference/diagnostics/lists.rs); reference excludes final member; standard coverage unverified |
| `eq-ref` | `RuleId::EqRefSubject + EqRefPredicate + EqRefObject` | [equality kernel](../src/inference/same_as.rs); indexed native execution; shared definition is semantic source |
| `eq-rep-o` | `RuleId::EqRepO` | [equality kernel](../src/inference/same_as.rs); indexed native execution; shared definition is semantic source |
| `eq-rep-p` | `RuleId::EqRepP` | [equality kernel](../src/inference/same_as.rs); indexed native execution; shared definition is semantic source |
| `eq-rep-s` | `RuleId::EqRepS` | [equality kernel](../src/inference/same_as.rs); indexed native execution; shared definition is semantic source |
| `eq-sym` | `RuleId::EqSym` | [equality kernel](../src/inference/same_as.rs); indexed native execution; shared definition is semantic source |
| `eq-trans` | `RuleId::EqTrans` | [equality kernel](../src/inference/same_as.rs); indexed native execution; shared definition is semantic source |
| `prp-adp` | Not a portable clause | [local binding](../src/inference/diagnostics/lists.rs); reference excludes final member; standard coverage unverified |
| `prp-ap` | Not a portable clause | [local binding](../src/inference/axioms.rs); fixed annotation vocabulary; standard coverage unverified |
| `prp-asyp` | Not a portable clause | [local binding](../src/inference/diagnostics/relations.rs); named/blank property diagnostics; standard coverage unverified |
| `prp-dom` | `RuleId::Rdfs2` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `prp-eqp1` | `RuleId::PrpEqp1` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `prp-eqp2` | `RuleId::PrpEqp2` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `prp-fp` | `RuleId::PrpFp` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `prp-ifp` | `RuleId::PrpIfp` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `prp-inv1` | `RuleId::PrpInv1` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `prp-inv2` | `RuleId::PrpInv2` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `prp-irp` | Not a portable clause | [local binding](../src/inference/diagnostics/relations.rs); named/blank property diagnostics; standard coverage unverified |
| `prp-key` | Not a portable clause | [local binding](../src/inference/traversal.rs); strict key-list kernel; standard coverage unverified |
| `prp-npa1` | Not a portable clause | [local binding](../src/inference/diagnostics/restrictions.rs); negative object assertion messages; standard coverage unverified |
| `prp-npa2` | Not a portable clause | [local binding](../src/inference/diagnostics/restrictions.rs); negative value assertion messages; standard coverage unverified |
| `prp-pdw` | Not a portable clause | [local binding](../src/inference/diagnostics/relations.rs); property conflict messages; standard coverage unverified |
| `prp-rng` | `RuleId::Rdfs3` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `prp-spo1` | `RuleId::Rdfs7` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `prp-spo2` | Not a portable clause | [local binding](../src/inference/traversal.rs); strict property-chain list kernel; standard coverage unverified |
| `prp-symp` | `RuleId::PrpSymp` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `prp-trp` | `RuleId::PrpTrp` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-avf1` | `RuleId::ScmAvf1` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-avf2` | `RuleId::ScmAvf2` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-cls` | `RuleId::ScmClsSelfSubclass / SelfEquivalent / Thing / Nothing` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-dom1` | `RuleId::ScmDom1` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-dom2` | `RuleId::ScmDom2` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-dp` | `RuleId::ScmDpSelfSubproperty + SelfEquivalent` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-eqc1` | `RuleId::ScmEqc1Forward + Reverse` | [shared adapter](../src/inference/shared.rs); versioned guarded `Owlrl762RuleId` binding differs from unrestricted standard |
| `scm-eqc2` | `RuleId::ScmEqc2` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-eqp1` | `RuleId::ScmEqp1Forward + Reverse` | [shared adapter](../src/inference/shared.rs); versioned guarded `Owlrl762RuleId` binding differs from unrestricted standard |
| `scm-eqp2` | `RuleId::ScmEqp2` | [shared adapter](../src/inference/shared.rs); versioned guarded `Owlrl762RuleId` binding differs from unrestricted standard |
| `scm-hv` | `RuleId::ScmHv` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-int` | `ListRuleId::ScmInt` | [shared adapter](../src/inference/shared.rs); auxiliary membership supplied by native paths |
| `scm-op` | `RuleId::ScmOpSelfSubproperty + SelfEquivalent` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-rng1` | `RuleId::ScmRng1` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-rng2` | `RuleId::ScmRng2` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-sco` | `RuleId::Rdfs11` | [shared adapter](../src/inference/shared.rs); versioned guarded `Owlrl762RuleId` binding differs from unrestricted standard |
| `scm-spo` | `RuleId::Rdfs5` | [shared adapter](../src/inference/shared.rs); versioned guarded `Owlrl762RuleId` binding differs from unrestricted standard |
| `scm-svf1` | `RuleId::ScmSvf1` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-svf2` | `RuleId::ScmSvf2` | [shared adapter](../src/inference/shared.rs); selected directly or through equivalent schema consequences |
| `scm-uni` | `ListRuleId::ScmUni` | [shared adapter](../src/inference/shared.rs); auxiliary membership supplied by native paths |

## Additional RDF/RDFS catalogue

The portable fourteen-entry catalogue contains `Rdf1`, `Rdfs2`, `Rdfs3`,
`Rdfs4a`, `Rdfs4b`, `Rdfs5`, `Rdfs6`, `Rdfs7`, `Rdfs8`, `Rdfs9`, `Rdfs10`,
`Rdfs11`, `Rdfs12`, and `Rdfs13`. Consumer selection is explicit in
[shared.rs](../src/inference/shared.rs); initial resource seeding is in
[axioms.rs](../src/inference/axioms.rs). These are not full RDFS: vocabulary
axioms/options, container seeding, datatype interpretation and entailment-query
contracts need separate coverage.
See https://www.w3.org/TR/rdf11-mt/#patterns-of-rdfs-entailment.

## Datatype and profile boundaries

| Operation | Shared foundation | Consumer / remaining gap |
| --- | --- | --- |
| Finite mathematical equality | `ExactNumber` | #86 already uses exact numeric keys for RDFS Python-value substitution; do not duplicate it |
| Literal object substitution | `LiteralRuleId::RdfsValueSubstitution` | [literal_values.rs](../src/inference/literal_values.rs) supplies the selected value-class relation |
| Standard finite numeric identity | Proposed rdf-reasoning PR #25, not published/adopted here | Separate datatype spaces and floating signed zeros; not a replacement for compatibility keys |
| Non-finite standard identity | Not provided by the finite primitive | Explicitly reject unsupported construction; requires its own complete policy |
| Datatype membership / lexical mapping | No complete portable implementation | [local converters](../src/inference/diagnostic_datatypes.rs) are Python profiles, not a standard datatype map |
| Default value truthiness / zero | No standard-rule claim | [truthiness](../src/inference/diagnostic_datatypes/truthiness.rs) and cardinality are distinct compatibility operations |
| Complete standard profiles | Rejected by portable preset contract | Do not infer full conformance from a runtime mode name |

The standard numeric boundary follows OWL 2 syntax section 4.2:
https://www.w3.org/TR/2012/REC-owl2-syntax-20121211/#Floating-Point_Numbers.
The existing mathematical key deliberately merges signed zero and cross-space
magnitudes. Standard identity must not reuse it without a value-domain contract.
The new finite primitive is a prerequisite, not implemented dt-eq/dt-diff execution.

## Evidence and next work

Existing evidence includes [shared clause tests](../tests/test_shared_rules.py),
[generalized parity](../tests/test_generalized_owl_parity.py),
[numeric substitution](../tests/test_numeric_substitution.py),
[datatype/reference tests](../tests/test_numeric_value_reference.py),
[diagnostics](../tests/test_generalized_diagnostics.py), and
[list/schema cases](../tests/test_generalized_schema.py). Those scopes are finite;
none supplies all standard membership, facet, malformed-list, false-head,
non-finite-value or direct-semantics obligations.

Next shared semantic work must select an explicit standard datatype domain,
provide checked transport and independent value/membership fixtures, then bind
its operations through backend capabilities. Reuse those operations in forward
and backward callers. Keep private Legra storage/execution separate, and do not
add more consumer-owned standard rule definitions to close catalogue gaps.
