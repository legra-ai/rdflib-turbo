//! Compare one complete immutable unscheduled round, not only fixed points.
use oxrdf::GeneralizedDataset;
use std::collections::HashSet;

use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryResults, SparqlEvaluator};

use super::super::{
    Graph, OWL_RULES, PREFIXES, Pending, apply_rules, axioms, intersections, scheduling::Schedule,
    traversal,
};
use super::{CompiledRule, oracle};
use crate::Result;

fn node(name: &str) -> NamedNode {
    NamedNode::new(name).unwrap()
}

fn fixture() -> GeneralizedDataset {
    let rdf = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
    let rdfs = "http://www.w3.org/2000/01/rdf-schema#";
    let owl = "http://www.w3.org/2002/07/owl#";
    // bounded: fixed portable-rule, equality, restriction, list/key/chain fixture.
    let mut facts: Vec<(NamedOrBlankNode, NamedNode, Term)> = Vec::new();
    for (subject, class) in [
        ("urn:property", format!("{rdf}Property")),
        ("urn:rdfs-class", format!("{rdfs}Class")),
        ("urn:owl-class", format!("{owl}Class")),
        ("urn:object-property", format!("{owl}ObjectProperty")),
        ("urn:data-property", format!("{owl}DatatypeProperty")),
    ] {
        facts.push((
            node(subject).into(),
            node(&format!("{rdf}type")),
            node(&class).into(),
        ));
    }
    for (subject, predicate, object) in [
        ("urn:property", format!("{rdfs}domain"), "urn:rdfs-class"),
        ("urn:property", format!("{rdfs}range"), "urn:Range"),
        ("urn:alice", "urn:property".into(), "urn:bob"),
        ("urn:alice", format!("{owl}sameAs"), "urn:alias"),
        ("urn:alias", format!("{owl}sameAs"), "urn:third"),
        ("urn:Union", format!("{owl}unionOf"), "urn:list"),
        (
            "urn:Intersection",
            format!("{owl}intersectionOf"),
            "urn:list",
        ),
        ("urn:Enum", format!("{owl}oneOf"), "urn:list"),
        ("urn:list", format!("{rdf}first"), "urn:A"),
        ("urn:list", format!("{rdf}rest"), "urn:tail"),
        ("urn:tail", format!("{rdf}first"), "urn:B"),
        ("urn:tail", format!("{rdf}rest"), &format!("{rdf}nil")),
        ("urn:x", format!("{rdf}type"), "urn:A"),
        ("urn:x", format!("{rdf}type"), "urn:B"),
        (
            "urn:Chain",
            format!("{owl}propertyChainAxiom"),
            "urn:chain-list",
        ),
        ("urn:chain-list", format!("{rdf}first"), "urn:p1"),
        ("urn:chain-list", format!("{rdf}rest"), "urn:chain-tail"),
        ("urn:chain-tail", format!("{rdf}first"), "urn:p2"),
        ("urn:chain-tail", format!("{rdf}rest"), &format!("{rdf}nil")),
        ("urn:a", "urn:p1".into(), "urn:b"),
        ("urn:b", "urn:p2".into(), "urn:c"),
        ("urn:KeyClass", format!("{owl}hasKey"), "urn:key-list"),
        ("urn:key-list", format!("{rdf}first"), "urn:key"),
        ("urn:key-list", format!("{rdf}rest"), &format!("{rdf}nil")),
        ("urn:k1", format!("{rdf}type"), "urn:KeyClass"),
        ("urn:k2", format!("{rdf}type"), "urn:KeyClass"),
    ] {
        facts.push((node(subject).into(), node(&predicate), node(object).into()));
    }
    facts.push((
        BlankNode::new("blank-class").unwrap().into(),
        node(&format!("{rdf}type")),
        node(&format!("{owl}Class")).into(),
    ));
    for lexical in ["1", "01"] {
        facts.push((
            BlankNode::new("blank-instance").unwrap().into(),
            node("urn:property"),
            Literal::new_typed_literal(lexical, node("http://www.w3.org/2001/XMLSchema#integer"))
                .into(),
        ));
    }
    for subject in ["urn:k1", "urn:k2"] {
        facts.push((
            node(subject).into(),
            node("urn:key"),
            Literal::new_language_tagged_literal("exact", "en")
                .unwrap()
                .into(),
        ));
    }
    let scopes = [
        GraphName::DefaultGraph,
        node("urn:scope").into(),
        BlankNode::new_unchecked("urn:scope").into(),
    ];
    let mut data = GeneralizedDataset::new();
    for (index, (subject, predicate, object)) in facts.into_iter().enumerate() {
        data.insert(&GeneralizedQuad::new(
            subject,
            predicate,
            object,
            scopes[index % scopes.len()].clone(),
        ));
    }
    // Duplicate SPO across graph kinds must remain a single logical union fact.
    for scope in scopes {
        data.insert(&GeneralizedQuad::new(
            node("urn:property"),
            node(&format!("{rdf}type")),
            node(&format!("{rdf}Property")),
            scope,
        ));
    }
    data
}

struct View<'a> {
    data: &'a GeneralizedDataset,
    scope: Option<GraphName>,
}

impl<'data> Graph for View<'data> {
    fn quads<'a>(
        &'a self,
        s: Option<TermRef<'_>>,
        p: Option<TermRef<'_>>,
        o: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'data> {
        let mut previous = None; // bounded: one preceding SPO in native graph-last order.
        self.data
            .quads_for_pattern(s, p, o, self.scope.as_ref().map(GraphName::as_ref))
            .filter(move |quad| {
                let triple = GeneralizedTripleRef {
                    subject: quad.subject,
                    predicate: quad.predicate,
                    object: quad.object,
                };
                previous.replace(triple) != Some(triple)
            })
            .map(|quad| {
                Ok(GeneralizedQuad {
                    graph_name: GraphName::DefaultGraph,
                    ..quad.into_owned()
                })
            })
    }

    fn query(&self, mut query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        match &self.scope {
            Some(scope) => query.dataset_mut().set_default_graph(vec![scope.clone()]),
            None => query.dataset_mut().set_default_graph_as_union(),
        }
        Ok(query.on_queryable_dataset(self.data).execute()?)
    }

    fn inference_heads(
        &self,
        mut query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        match &self.scope {
            Some(scope) => query.dataset_mut().set_default_graph(vec![scope.clone()]),
            None => query.dataset_mut().set_default_graph_as_union(),
        }
        Ok(query
            .on_queryable_dataset(self.data)
            .execute_inference_construct()?)
    }

    fn cleanup_blank_predicates(&mut self) -> Result<()> {
        panic!("immutable kernel fixture must not postprocess storage")
    }

    fn insert(&mut self, _: GeneralizedQuad) -> Result<()> {
        panic!("immutable round must not publish")
    }
}

fn round(
    view: &View<'_>,
    mut queries: Vec<PreparedSparqlQuery>,
    mode: &str,
) -> Result<HashSet<GeneralizedQuad>> {
    let owl = mode != "rdfs";
    if owl {
        queries.extend(OWL_RULES.iter().map(|(head, body)| {
            SparqlEvaluator::new()
                .parse_query(&format!(
                    "{PREFIXES} CONSTRUCT {{ {head} }} WHERE {{ {body} }}"
                ))
                .unwrap()
        }));
    }
    let mut pending = Pending::default();
    if mode != "owlrl" {
        for quad in axioms::resources(view) {
            pending.push(view, quad?)?;
        }
    }
    apply_rules(view, &queries, &mut pending, owl)?;
    if owl {
        intersections::additions(view, &mut pending)?;
        traversal::chains(
            view,
            &mut pending,
            &crate::inference::diagnostics::rdf_list::ListWitnesses::new(4300),
        )?;
        traversal::keys(
            view,
            &mut pending,
            &crate::inference::diagnostics::rdf_list::ListWitnesses::new(4300),
        )?;
    }
    Ok(pending.into_iter().collect())
}

fn compare(data: &GeneralizedDataset, scope: Option<GraphName>, require_success: bool) {
    let view = View { data, scope };
    for mode in ["rdfs", "owlrl", "both"] {
        let expected = round(&view, oracle::queries(mode).unwrap(), mode);
        let actual = round(&view, CompiledRule::for_mode(mode).unwrap(), mode);
        match (actual, expected) {
            (Ok(actual), Ok(expected)) => {
                assert_eq!(actual, expected, "{mode} {:?}", view.scope);
                assert!(!actual.is_empty());
            }
            (Err(actual), Err(expected)) if !require_success => {
                assert_eq!(actual.to_string(), expected.to_string());
                assert!(actual.to_string().contains("Malformed RDF list"));
            }
            (actual, expected) => panic!(
                "round result mismatch: {mode} {:?}: {actual:?} vs {expected:?}",
                view.scope
            ),
        }
    }
}

#[test]
fn grouped_catalogues_match_frozen_full_immutable_round_for_all_modes_and_scopes() {
    let data = fixture();
    let before = data.clone(); // bounded: this fixed synthetic oracle fixture only.
    compare(&data, None, true);
    for scope in [
        GraphName::DefaultGraph,
        node("urn:scope").into(),
        BlankNode::new_unchecked("urn:scope").into(),
    ] {
        // Split lists are malformed only in the selected view: preserve the
        // original hard error instead of borrowing their missing tail elsewhere.
        compare(&data, Some(scope.clone()), false);
        // bounded: one complete copy of the fixed fixture, confined to the
        // selected context, also verifies successful list/key/chain phases.
        let complete = GeneralizedDataset::from_iter(data.iter().map(|quad| GeneralizedQuad {
            graph_name: scope.clone(),
            ..quad.into_owned()
        }));
        compare(&complete, Some(scope), true);
    }
    assert_eq!(data, before);
}

#[test]
fn scheduled_groups_reactivate_all_heads_for_late_class_and_equivalence_facts() {
    let rdf_type = node("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let equivalent = node("http://www.w3.org/2002/07/owl#equivalentClass");
    for mode in ["rdfs", "owlrl", "both"] {
        let mut data = GeneralizedDataset::new();
        let mut schedule = Schedule::for_mode(mode).unwrap();
        let queries = oracle::queries(mode).unwrap();
        // bounded: four fixed late publications after the empty bootstrap round.
        let additions = [
            GeneralizedQuad::new(
                node("urn:A"),
                rdf_type.clone(),
                node("http://www.w3.org/2000/01/rdf-schema#Class"),
                GraphName::DefaultGraph,
            ),
            GeneralizedQuad::new(
                node("urn:x"),
                rdf_type.clone(),
                node("urn:A"),
                GraphName::DefaultGraph,
            ),
            GeneralizedQuad::new(
                node("urn:A"),
                equivalent.clone(),
                node("urn:A"),
                GraphName::DefaultGraph,
            ),
            GeneralizedQuad::new(
                node("urn:A"),
                equivalent.clone(),
                node("urn:B"),
                GraphName::DefaultGraph,
            ),
        ];
        for round in 0usize..32 {
            let view = View {
                data: &data,
                scope: Some(GraphName::DefaultGraph),
            };
            let mut expected = Pending::default();
            apply_rules(&view, &queries, &mut expected, false).unwrap();
            let mut actual = Pending::default();
            schedule.apply(&view, &mut actual).unwrap();
            assert_eq!(actual, expected, "{mode} late grouped round {round}");
            let external = round
                .checked_sub(1)
                .and_then(|index| additions.get(index))
                .cloned();
            if actual.is_empty() && external.is_none() && round > additions.len() {
                break;
            }
            let mut dirty = 0;
            for quad in actual.into_iter().chain(external) {
                dirty |= schedule.changed(&quad.predicate);
                data.insert(quad.as_ref());
            }
            schedule.advance(dirty);
            assert!(round < 31, "late fixture did not converge");
        }
        if mode != "rdfs" {
            assert!(
                data.contains(
                    GeneralizedQuad::new(
                        node("urn:x"),
                        rdf_type.clone(),
                        node("urn:B"),
                        GraphName::DefaultGraph
                    )
                    .as_ref()
                )
            );
        }
    }
}
