use super::StorageError;
use super::numeric_encoder::{Decoder, EncodedQuad, StrHash, StrLookup};
use crate::model::{
    BlankNode, GeneralizedQuad, GeneralizedTermPosition, GraphName, Literal, NamedNode, Term,
};
use std::cell::Cell;
use std::io;

struct ReadProbe {
    reads: Cell<usize>,
}

impl StrLookup for ReadProbe {
    fn get_str(&self, _: &StrHash) -> Result<Option<String>, StorageError> {
        self.reads.set(self.reads.get() + 1);
        Err(io::Error::other("later position dictionary read").into())
    }
}

#[test]
fn generalized_strict_decoder_rejects_first_position_before_later_dictionary_reads() {
    // bounded: two positional failures with inline terms requiring no dictionary lookup.
    let cases = [
        (
            Term::from(Literal::new_simple_literal("subject")),
            Term::from(NamedNode::new("urn:predicate").unwrap()),
            GeneralizedTermPosition::Subject,
        ),
        (
            BlankNode::new("subject").unwrap().into(),
            Literal::new_simple_literal("predicate").into(),
            GeneralizedTermPosition::Predicate,
        ),
    ];
    for (subject, predicate, expected) in cases {
        let quad = GeneralizedQuad::new(
            subject,
            predicate,
            NamedNode::new("urn:object").unwrap(),
            GraphName::DefaultGraph,
        );
        let probe = ReadProbe {
            reads: Cell::new(0),
        };
        let error = probe
            .decode_quad(&EncodedQuad::from(quad.as_ref()))
            .unwrap_err();
        match error {
            StorageError::GeneralizedNarrowing(error) => assert_eq!(error.position(), expected),
            error => panic!("expected first positional narrowing, got {error}"),
        }
        assert_eq!(probe.reads.get(), 0);
    }
}

#[test]
fn generalized_decoder_propagates_actual_dictionary_read_failure() {
    let quad = GeneralizedQuad::new(
        BlankNode::new("subject").unwrap(),
        Literal::new_simple_literal("predicate"),
        NamedNode::new("urn:object").unwrap(),
        GraphName::DefaultGraph,
    );
    let probe = ReadProbe {
        reads: Cell::new(0),
    };
    let error = probe
        .decode_generalized_quad(&EncodedQuad::from(quad.as_ref()))
        .unwrap_err();
    assert!(matches!(&error, StorageError::Io(_)));
    assert_eq!(error.to_string(), "later position dictionary read");
    assert_eq!(probe.reads.get(), 1);
}
