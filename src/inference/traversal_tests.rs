use super::*;
use oxigraph::io::{RdfFormat, RdfParser};

fn dataset(source: &str) -> Dataset {
    RdfParser::from_format(RdfFormat::Turtle)
        .for_slice(source.as_bytes())
        .map(|q| q.unwrap())
        .collect()
}

fn checked_list(data: &Dataset, head: &Term) -> Result<Vec<NamedNode>> {
    let mut data = data.clone();
    list(&data.insert_transaction(), head, "members must be IRIs")
}

#[test]
fn rdf_lists_validate_every_link_and_reject_ambiguous_or_cyclic_structure() {
    let head: Term = NamedNode::new("urn:head").unwrap().into();
    assert!(
        checked_list(&Dataset::new(), &iri(RDF, "nil").into())
            .unwrap()
            .is_empty()
    );
    let valid = dataset(
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .",
    );
    assert_eq!(
        checked_list(&valid, &head).unwrap(),
        vec![NamedNode::new("urn:a").unwrap()]
    );
    for source in [
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a> .",
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a>, <urn:b>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <http://www.w3.org/1999/02/22-rdf-syntax-ns#nil> .",
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <urn:a>, <urn:b> .",
    ] {
        assert!(
            checked_list(&dataset(source), &head)
                .unwrap_err()
                .to_string()
                .contains("Malformed RDF list")
        );
    }
    let cycle = dataset(
        "<urn:head> <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:a>; <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> <urn:head> .",
    );
    assert!(
        checked_list(&cycle, &head)
            .unwrap_err()
            .to_string()
            .contains("Cyclic RDF list")
    );
    assert!(checked_list(&valid, &Literal::new_simple_literal("head").into()).is_err());
}
