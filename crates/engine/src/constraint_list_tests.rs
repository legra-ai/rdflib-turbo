//! Stock cursor and read-failure contracts independent of path-list syntax.
use super::*;

#[test]
fn stock_python_literal_values_are_distinct_from_ebv() {
    for (kind, value, expected) in [
        ("string", "", false),
        ("string", " ", true),
        ("boolean", "false", false),
        ("boolean", "invalid", false),
        ("integer", "0", false),
        ("integer", "-0_0", false),
        ("integer", "invalid", true),
        ("integer", "٠", false),
        ("integer", "١", true),
        ("decimal", "0.00", false),
        ("decimal", "NaN", true),
        ("decimal", "sNaN", true),
        ("decimal", "invalid", true),
        ("double", "0.0", false),
        ("double", "NaN", true),
        ("double", "invalid", true),
        ("base64Binary", " ", false),
        ("base64Binary", "!", false),
        ("base64Binary", "=", false),
        ("base64Binary", "A", true),
        ("base64Binary", "AA==", true),
        ("base64Binary", "é", true),
        ("hexBinary", "00", true),
        ("hexBinary", " ", true),
        ("duration", "PT0S", false),
        ("duration", "PT0.0000006S", true),
    ] {
        let literal =
            Literal::new_typed_literal(value, iri("http://www.w3.org/2001/XMLSchema#", kind));
        assert_eq!(
            truthiness::truthy(&literal.into()),
            expected,
            "{kind} {value:?}"
        );
    }
    for value in ["[]", "{}", "false", "0"] {
        let literal = Literal::new_typed_literal(value, iri(RDF, "JSON"));
        assert!(truthiness::truthy(&literal.into()));
    }
}

struct FailedRead {
    predicate: NamedNode,
}

impl GraphRead for FailedRead {
    fn matching_quads<'a>(
        &'a self,
        _subject: Option<TermRef<'_>>,
        predicate: Option<TermRef<'_>>,
        _object: Option<TermRef<'_>>,
        _graph: Option<GraphNameRef<'_>>,
    ) -> impl Iterator<Item = Result<GeneralizedQuad>> + use<'a> {
        let fail = predicate == Some(self.predicate.as_ref().into());
        std::iter::once(Err(Error::Runtime("collection source failed".into())))
            .filter(move |_| fail)
    }

    fn named_graphs(&self) -> impl Iterator<Item = Result<NamedOrBlankNode>> + '_ {
        std::iter::empty()
    }

    fn contains_named_graph(&self, _graph: &NamedOrBlankNode) -> Result<bool> {
        Ok(false)
    }
}

#[test]
fn collection_reads_propagate_first_and_rest_source_errors() {
    let head: Term = NamedNode::new("urn:membership:head").unwrap().into();
    for kind in ["first", "rest"] {
        let source = FailedRead {
            predicate: iri(RDF, kind),
        };
        assert!(
            matches!(read(&source, &head, Default::default()), Err(Error::Runtime(message)) if message == "collection source failed")
        );
    }
}

#[test]
fn falsey_cursor_does_not_read_its_descriptions() {
    let source = FailedRead {
        predicate: iri(RDF, "first"),
    };
    for head in [
        Term::NamedNode(NamedNode::new_unchecked("")),
        Term::BlankNode(BlankNode::new_unchecked("")),
        Literal::new_simple_literal("").into(),
        Literal::new_typed_literal("0", iri("http://www.w3.org/2001/XMLSchema#", "integer")).into(),
    ] {
        assert!(read(&source, &head, Default::default()).unwrap().is_empty());
    }
}

#[test]
fn multiple_first_values_use_one_permitted_witness() {
    let head = NamedNode::new("urn:membership:head").unwrap();
    let values = [
        NamedNode::new("urn:a").unwrap(),
        NamedNode::new("urn:b").unwrap(),
    ];
    let source = Dataset::from_iter(values.iter().map(|value| {
        Quad::new(
            head.clone(),
            iri(RDF, "first"),
            value.clone(),
            GraphName::DefaultGraph,
        )
    }));
    let actual = read(&source, &head.into(), Default::default()).unwrap();
    assert_eq!(actual.len(), 1);
    assert!(
        values
            .iter()
            .any(|value| actual[0] == Term::from(value.clone()))
    );
}

#[test]
fn host_decimal_none_does_not_fall_back_to_newer_rust_digits() {
    let host = crate::constraints::DecimalDigitPolicy::new(|_| None);
    for ch in ['\u{11bf0}', '\u{10d40}', '\u{1e5f1}'] {
        assert_eq!(crate::constraints::decimal_digit(ch), Some(0));
        for kind in ["integer", "decimal", "double"] {
            let term =
                Literal::new_typed_literal(ch.to_string(), iri(crate::graph::XSD, kind)).into();
            assert!(!truthiness::truthy_with_policy(&term, Default::default()));
            assert!(truthiness::truthy_with_policy(&term, host));
        }
    }
    assert_eq!(host.decimal('0'), Some(0));
    assert_eq!(host.decimal('9'), Some(9));
    assert_eq!(host.decimal('٠'), None);
}
