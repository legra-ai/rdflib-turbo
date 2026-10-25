use super::*;
use oxigraph::io::{RdfFormat, RdfParser};
use oxrdf::GeneralizedDataset;

fn dataset(source: &str) -> GeneralizedDataset {
    RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(source.as_bytes())
        .map(|q| q.unwrap())
        .collect()
}

fn checked_list(data: &GeneralizedDataset, head: &Term) -> Result<Vec<Term>> {
    let mut data = data.clone();
    list(&data.transaction(), head, &ListWitnesses::new(4300))
}

#[test]
fn rdf_lists_follow_reference_first_witness_and_reject_cycles() {
    let head: Term = NamedNode::new("urn:head").unwrap().into();
    assert!(
        checked_list(&GeneralizedDataset::new(), &iri(RDF, "nil").into())
            .unwrap()
            .is_empty()
    );
    let valid = dataset(
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .",
    );
    assert_eq!(
        checked_list(&valid, &head).unwrap(),
        vec![Term::from(NamedNode::new("urn:a").unwrap())]
    );
    for source in [
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a> .",
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <urn:a>, <urn:b> .",
    ] {
        assert_eq!(
            checked_list(&dataset(source), &head).unwrap(),
            vec![Term::from(NamedNode::new("urn:a").unwrap())]
        );
    }
    let cycle = dataset(
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <urn:head> .",
    );
    assert!(
        checked_list(&cycle, &head)
            .unwrap_err()
            .to_string()
            .contains("List contains a recursive rdf:rest reference")
    );
    assert!(
        checked_list(&valid, &Literal::new_simple_literal("head").into())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn ambiguous_list_selects_one_permitted_first_witness_without_rejection() {
    let input = dataset(
        "<urn:left> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a>, <urn:b> .
         <urn:right> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:b>, <urn:a> .",
    );
    // Graph.items uses Graph.value(any=True): ambiguous cells promise any
    // single witness, not Memory-store insertion order across implementations.
    let allowed = [
        Term::from(NamedNode::new("urn:a").unwrap()),
        Term::from(NamedNode::new("urn:b").unwrap()),
    ];
    for head in ["left", "right"] {
        let values = checked_list(
            &input,
            &NamedNode::new(format!("urn:{head}")).unwrap().into(),
        )
        .unwrap();
        assert_eq!(values.len(), 1);
        assert!(allowed.contains(&values[0]));
    }
}
