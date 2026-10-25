use super::backend::Graph;
use super::{PREFIXES, Pending, intersections};
use crate::Result;
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::*;
use oxigraph::sparql::{PreparedSparqlQuery, QueryExplanation, QueryResults, SparqlEvaluator};
use oxrdf::GeneralizedDataset;
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::collections::HashSet;

const ORIGINAL_PATTERN: &str = "?c owl:intersectionOf ?list . ?x a ?any FILTER NOT EXISTS { ?list rdf:rest*/rdf:first ?member FILTER NOT EXISTS { ?x a ?member } }";

fn dataset(source: &str) -> GeneralizedDataset {
    RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(source.as_bytes())
        .map(|quad| quad.unwrap())
        .collect()
}

#[derive(Deserialize)]
struct Explanation {
    plan: Plan,
}

#[derive(Deserialize)]
struct Plan {
    #[serde(rename = "number of results")]
    results: u64,
    children: Vec<Self>,
}

impl Plan {
    fn largest_intermediate(&self) -> u64 {
        self.children
            .iter()
            .map(Self::largest_intermediate)
            .fold(self.results, u64::max)
    }
}

fn evaluate(data: &GeneralizedDataset, pattern: &str) -> (HashSet<Triple>, Plan) {
    let (result, explanation) = SparqlEvaluator::new()
        .parse_query(&format!(
            "{PREFIXES} CONSTRUCT {{ ?x a ?c }} WHERE {{ {pattern} }}"
        ))
        .unwrap()
        .on_queryable_dataset(data)
        .compute_statistics()
        .explain();
    let QueryResults::Graph(rows) = result.unwrap() else {
        panic!("expected constructed triples");
    };
    // bounded: each synthetic test has at most 65 possible inferred type triples.
    let triples = rows.map(|triple| triple.unwrap()).collect();
    // bounded: explanation of one fixed built-in rule, independent of data size.
    let mut bytes = Vec::new();
    explanation.write_in_json(&mut bytes).unwrap();
    let explanation: Explanation = serde_json::from_slice(&bytes).unwrap();
    (triples, explanation.plan)
}

struct MeasuredGraph<'a> {
    data: &'a GeneralizedDataset,
    rows: Cell<u64>,
    explanations: RefCell<Vec<QueryExplanation>>,
}

impl<'data> Graph for MeasuredGraph<'data> {
    fn quads<'a>(
        &'a self,
        subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        object: Option<TermRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a, 'data> {
        self.data
            .quads_for_pattern(subject, predicate, object, Some(GraphNameRef::DefaultGraph))
            .inspect(|_| self.rows.set(self.rows.get() + 1))
            .map(|quad| Ok(quad.into_owned()))
    }
    fn query(&self, query: PreparedSparqlQuery) -> Result<QueryResults<'_>> {
        let (result, explanation) = query
            .on_queryable_dataset(self.data)
            .compute_statistics()
            .explain();
        self.explanations.borrow_mut().push(explanation);
        Ok(result?)
    }

    fn inference_heads(
        &self,
        query: PreparedSparqlQuery,
    ) -> Result<oxigraph::sparql::QueryTripleIter<'_, GeneralizedTriple>> {
        let (result, explanation) = query
            .on_queryable_dataset(self.data)
            .compute_statistics()
            .explain_inference_construct();
        self.explanations.borrow_mut().push(explanation);
        Ok(result?)
    }
    fn cleanup_blank_predicates(&mut self) -> Result<()> {
        panic!("immutable kernel fixture must not postprocess storage")
    }

    fn insert(&mut self, _: GeneralizedQuad) -> Result<()> {
        panic!("inference candidates must remain staged");
    }
}

fn current(data: &GeneralizedDataset) -> (HashSet<Triple>, u64) {
    let measured = MeasuredGraph {
        data,
        rows: Cell::new(0),
        explanations: RefCell::new(Vec::new()),
    };
    let mut additions = Pending::default();
    intersections::additions(&measured, &mut additions).unwrap();
    // bounded: synthetic fixtures produce at most 65 distinct type additions.
    let triples = additions
        .into_iter()
        .map(|quad| {
            Triple::new(
                NamedOrBlankNode::try_from(quad.subject).expect("ordinary subject"),
                NamedNode::try_from(quad.predicate).expect("ordinary predicate"),
                quad.object,
            )
        })
        .collect();
    let mut work = measured.rows.get();
    for explanation in measured.explanations.into_inner() {
        // bounded: one fixed built-in rule's execution plan.
        let mut bytes = Vec::new();
        explanation.write_in_json(&mut bytes).unwrap();
        let report: Explanation = serde_json::from_slice(&bytes).unwrap();
        work = work.max(report.plan.largest_intermediate());
    }
    (triples, work)
}

#[test]
fn sparse_intersections_do_not_cross_every_class_with_every_typed_subject() {
    for size in [16, 64] {
        let mut source = String::new();
        for index in 0..size {
            source.push_str(&format!(
                "<urn:C{index}> <http://www.w3.org/2002/07/owl#intersectionOf> <urn:list{index}> .\n\
                 <urn:list{index}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:M{index}> ;\n\
                 <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .\n\
                 <urn:x{index}> a <urn:Other> .\n"
            ));
        }
        source.push_str("<urn:accepted> a <urn:M0> .");
        let data = dataset(&source);
        let (expected, original) = evaluate(&data, ORIGINAL_PATTERN);
        let (actual, work) = current(&data);
        assert_eq!(actual, expected);
        assert_eq!(actual.len(), 1);
        assert!(original.largest_intermediate() >= size * (size + 1));
        assert!(
            work <= 4 * size + 4,
            "sparse intersection visited {work} rows for {size} classes"
        );
    }
}

#[test]
fn intersection_rewrite_preserves_empty_dangling_cyclic_and_branching_paths() {
    let prefix = "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
                  @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
                  <urn:x> a <urn:A>, <urn:B> . <urn:y> a <urn:A> .\n\
                  <urn:z> a <urn:C> .\n";
    for structure in [
        "<urn:I> owl:intersectionOf rdf:nil .",
        "<urn:I> owl:intersectionOf <urn:missing> .",
        "<urn:I> owl:intersectionOf \"literal\" .",
        "<urn:I> owl:intersectionOf <urn:list> . <urn:list> rdf:first <urn:A> .",
        "<urn:I> owl:intersectionOf <urn:list> . <urn:list> rdf:rest <urn:list> .",
        "<urn:I> owl:intersectionOf <urn:list> . <urn:list> rdf:first <urn:A> ; rdf:rest <urn:list> .",
        "<urn:I> owl:intersectionOf <urn:list> . <urn:list> rdf:first <urn:A>, <urn:B> ; rdf:rest rdf:nil .",
        "<urn:I> owl:intersectionOf <urn:list> . <urn:list> rdf:first <urn:A> ; rdf:rest <urn:left>, <urn:right> . <urn:left> rdf:first <urn:B> . <urn:right> rdf:first <urn:C> .",
        "<urn:I> owl:intersectionOf (<urn:A> <urn:B>) .",
        "<urn:I> owl:intersectionOf (\"literal class\") . <urn:x> a \"literal class\" .",
    ] {
        let data = dataset(&format!("{prefix}{structure}"));
        assert_eq!(
            current(&data).0,
            evaluate(&data, ORIGINAL_PATTERN).0,
            "changed property-path semantics for {structure}"
        );
    }
}

#[test]
fn unrelated_intersections_do_not_multiply_a_fixed_individual_population() {
    let individuals = 64_u64;
    for classes in [8_u64, 16, 32] {
        let mut source = String::from(
            "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n",
        );
        for index in 0..classes {
            source.push_str(&format!(
                "<urn:C{index}> owl:intersectionOf (<urn:A{index}> <urn:B{index}>) .\n"
            ));
        }
        for index in 0..individuals {
            source.push_str(&format!("<urn:x{index}> a <urn:A0> .\n"));
            if index % 2 == 0 {
                source.push_str(&format!("<urn:x{index}> a <urn:B0> .\n"));
            }
        }
        let data = dataset(&source);
        let (actual, work) = current(&data);
        assert_eq!(actual.len() as u64, individuals / 2);
        for triple in actual {
            assert_eq!(triple.object, NamedNode::new("urn:C0").unwrap().into());
        }
        let budget = 8 * classes + 8 * individuals;
        assert!(
            work <= budget,
            "{classes} intersections visited {work} rows; budget {budget}"
        );
    }
}
