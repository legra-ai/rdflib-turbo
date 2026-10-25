use super::*;
use crate::path::{PATH_EVALUATIONS, Path};

#[test]
fn traversal_cache_retention_is_bounded() {
    let mut evaluation = Evaluation::new(15);
    let path = Path::OneOrMore(Box::new(Path::Predicate(iri("urn:", "p"))));
    let data = Dataset::new();
    for index in 0..1025 {
        let focus = Term::from(iri("urn:node:", &index.to_string()));
        assert!(evaluation.values(&path, &data, &focus).unwrap().is_empty());
        assert!(evaluation.path_values.len() <= 1024);
    }
    assert!(!evaluation.reserve_traversal(32769));
    evaluation.clear_traversals();
    assert!(evaluation.path_values.is_empty());
    assert_eq!(evaluation.cached_terms, 0);
}

#[test]
fn property_paths_are_reused_only_until_the_phase_changes() {
    let mut evaluation = Evaluation::new(15);
    let path = Path::OneOrMore(Box::new(Path::Predicate(iri("urn:", "next"))));
    let focus = Term::from(iri("urn:", "s"));
    let mut data = parse(
        "<urn:s> <urn:next> <urn:t> .",
        oxigraph::io::RdfFormat::Turtle,
        None,
    )
    .unwrap();
    PATH_EVALUATIONS.with(|count| count.set(0));
    assert_eq!(evaluation.values(&path, &data, &focus).unwrap().len(), 1);
    assert_eq!(evaluation.values(&path, &data, &focus).unwrap().len(), 1);
    assert_eq!(PATH_EVALUATIONS.with(std::cell::Cell::get), 1);
    data.insert(
        Quad::new(
            iri("urn:", "t"),
            iri("urn:", "next"),
            iri("urn:", "u"),
            GraphName::DefaultGraph,
        )
        .as_ref(),
    );
    evaluation.clear_traversals();
    assert_eq!(evaluation.values(&path, &data, &focus).unwrap().len(), 2);
    assert_eq!(PATH_EVALUATIONS.with(std::cell::Cell::get), 2);
}
