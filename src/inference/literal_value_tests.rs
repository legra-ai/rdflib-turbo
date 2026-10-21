use oxigraph::model::*;

use super::expand;

#[test]
fn rdfs_substitutes_equal_string_values_across_datatypes_and_languages() {
    let predicate = NamedNode::new("urn:label").unwrap();
    let xsd = "http://www.w3.org/2001/XMLSchema#";
    // bounded: the seven builtin string-valued term forms and two controls.
    let values = [
        Literal::new_simple_literal("same"),
        Literal::new_language_tagged_literal("same", "en").unwrap(),
        Literal::new_language_tagged_literal("same", "fr").unwrap(),
        Literal::new_typed_literal("same", NamedNode::new(format!("{xsd}string")).unwrap()),
        Literal::new_typed_literal("same", NamedNode::new(format!("{xsd}token")).unwrap()),
        Literal::new_typed_literal("same", NamedNode::new(format!("{xsd}anyURI")).unwrap()),
        Literal::new_typed_literal("same", NamedNode::new(format!("{xsd}language")).unwrap()),
        Literal::new_simple_literal("different"),
        Literal::new_typed_literal("same", NamedNode::new("urn:unknown-datatype").unwrap()),
    ];
    let mut data = Dataset::from_iter(values.iter().enumerate().map(|(index, value)| {
        Quad::new(
            NamedNode::new(format!("urn:s{index}")).unwrap(),
            predicate.clone(),
            value.clone(),
            GraphName::DefaultGraph,
        )
    }));
    expand(&mut data, "rdfs").unwrap();
    for (subject, _) in values.iter().enumerate() {
        for (replacement, value) in values.iter().enumerate() {
            let expected = subject == replacement || (subject < 7 && replacement < 7);
            let quad = Quad::new(
                NamedNode::new(format!("urn:s{subject}")).unwrap(),
                predicate.clone(),
                value.clone(),
                GraphName::DefaultGraph,
            );
            assert_eq!(data.contains(&quad), expected, "{quad}");
        }
    }
}
