//! Persistent generalized statements share the existing v1 relation and keys.

use super::binary_encoder::{
    QuadEncoding, write_gosp_quad, write_gpos_quad, write_gspo_quad, write_osp_quad,
    write_ospg_quad, write_pos_quad, write_posg_quad, write_spo_quad, write_spog_quad,
};
use super::numeric_encoder::EncodedQuad;
use crate::model::{
    BlankNode, GeneralizedQuad, GeneralizedQuadRef, GraphName, Literal, NamedNode, Quad, Term,
};
use crate::store::Store;
use std::collections::HashSet;

fn iri(value: &str) -> NamedNode {
    NamedNode::new(value).unwrap()
}

fn fixture() -> Vec<GeneralizedQuad> {
    // bounded: 27 synthetic rows, independent of any source relation.
    let terms = [
        Term::from(iri("urn:term")),
        BlankNode::new("term").unwrap().into(),
        Literal::new_typed_literal(
            "a long exact lexical value 0001",
            iri("urn:datatype:custom:long"),
        )
        .into(),
    ];
    let graphs = [
        GraphName::DefaultGraph,
        iri("urn:graph").into(),
        BlankNode::new("graph").unwrap().into(),
    ];
    let mut quads = Vec::new();
    for (subject_index, subject) in terms.iter().enumerate() {
        for (predicate_index, predicate) in terms.iter().enumerate() {
            for graph in &graphs {
                quads.push(GeneralizedQuad::new(
                    subject.clone(),
                    predicate.clone(),
                    terms[(subject_index + predicate_index) % terms.len()].clone(),
                    graph.clone(),
                ));
            }
        }
    }
    quads
}

fn verify_masks(store: &Store, expected: &[GeneralizedQuad]) {
    for probe in expected {
        for mask in 0..16 {
            let subject = (mask & 1 != 0).then(|| probe.subject.as_ref());
            let predicate = (mask & 2 != 0).then(|| probe.predicate.as_ref());
            let object = (mask & 4 != 0).then(|| probe.object.as_ref());
            let graph = (mask & 8 != 0).then(|| probe.graph_name.as_ref());
            // bounded: at most 27 synthetic rows in each comparison set.
            let actual: HashSet<_> = store
                .generalized_quads_for_pattern(subject, predicate, object, graph)
                .map(Result::unwrap)
                .collect();
            let expected: HashSet<_> = expected
                .iter()
                .filter(|quad| {
                    subject.is_none_or(|term| term == quad.subject.as_ref())
                        && predicate.is_none_or(|term| term == quad.predicate.as_ref())
                        && object.is_none_or(|term| term == quad.object.as_ref())
                        && graph.is_none_or(|term| term == quad.graph_name.as_ref())
                })
                .cloned()
                .collect();
            assert_eq!(actual, expected, "mask={mask}, probe={probe:?}");
        }
    }
    assert_eq!(store.len().unwrap(), expected.len());
    store.validate().unwrap();
}

#[test]
fn generalized_persistent_relation_reopens_and_retains_owned_snapshot_after_store_drop() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::open(directory.path()).unwrap();
    let original = fixture();
    for quad in &original {
        store.insert_generalized(quad).unwrap();
    }
    verify_masks(&store, &original);
    let extra = GeneralizedQuad::new(
        Literal::new_typed_literal(
            "new exact dictionary value beyond inline",
            iri("urn:datatype:new"),
        ),
        Literal::new_language_tagged_literal("predicate", "nl").unwrap(),
        iri("urn:new:object"),
        BlankNode::new("pending").unwrap(),
    );
    {
        let mut transaction = store.start_transaction().unwrap();
        transaction.remove_generalized(&original[0]);
        transaction.insert_generalized(&extra);
        assert_eq!(
            transaction
                .generalized_quads_for_pattern(
                    Some(extra.subject.as_ref()),
                    Some(extra.predicate.as_ref()),
                    None,
                    None,
                )
                .next()
                .unwrap()
                .unwrap(),
            extra
        );
        // Drop rolls back the generalized insert, removal and graph metadata.
    }
    verify_masks(&store, &original);
    let snapshot = store.read_snapshot();
    let rows = snapshot.generalized_quads_for_pattern(None, None, None, None);
    drop(snapshot);
    let mut transaction = store.start_transaction().unwrap();
    transaction.remove_generalized(&original[0]);
    transaction.insert_generalized(&extra);
    transaction.commit().unwrap();
    store.flush().unwrap();
    drop(store);
    // Decode after physical source handle drop, on another thread, including long lexical values.
    let observed: HashSet<_> = std::thread::spawn(move || {
        // bounded: exactly 27 rows in this pinned synthetic fixture.
        rows.map(Result::unwrap).collect::<HashSet<_>>()
    })
    .join()
    .unwrap();
    assert_eq!(observed, original.iter().cloned().collect());
    let reopened = Store::open(directory.path()).unwrap();
    // bounded: edit the same 27-row expected fixture, never a source relation.
    let mut expected = original;
    expected.remove(0);
    expected.push(extra);
    verify_masks(&reopened, &expected);
    assert_eq!(reopened.iter().count(), expected.len());
}

type KeyWriter = fn(&mut Vec<u8>, &EncodedQuad);

struct KeyCase {
    name: &'static str,
    write: KeyWriter,
    decode: QuadEncoding,
    order: &'static [usize],
    named: bool,
}

#[test]
fn generalized_promotion_preserves_existing_v1_persisted_key_bytes() {
    // Frozen v1 encodings: small blank/string payloads and the existing exact-mode
    // SipHash13 vector for urn:subject:7 (also asserted in lexical_hash_tests).
    let blank = [9, 102, 111, 111, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3];
    let node = [
        1, 0x99, 0xae, 0xf6, 0x47, 0xb1, 0x15, 0x00, 0x2f, 0x47, 0xfe, 0xb4, 0x1e, 0xd9, 0xd8,
        0x31, 0xc5,
    ];
    let literal = [16, 102, 111, 111, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3];
    let parts = [blank, node, literal, node];
    // bounded: all nine existing persisted key orders, at most 68 bytes per key.
    let cases = [
        KeyCase {
            name: "dspo",
            write: write_spo_quad,
            decode: QuadEncoding::Dspo,
            order: &[0, 1, 2],
            named: false,
        },
        KeyCase {
            name: "dpos",
            write: write_pos_quad,
            decode: QuadEncoding::Dpos,
            order: &[1, 2, 0],
            named: false,
        },
        KeyCase {
            name: "dosp",
            write: write_osp_quad,
            decode: QuadEncoding::Dosp,
            order: &[2, 0, 1],
            named: false,
        },
        KeyCase {
            name: "spog",
            write: write_spog_quad,
            decode: QuadEncoding::Spog,
            order: &[0, 1, 2, 3],
            named: true,
        },
        KeyCase {
            name: "posg",
            write: write_posg_quad,
            decode: QuadEncoding::Posg,
            order: &[1, 2, 0, 3],
            named: true,
        },
        KeyCase {
            name: "ospg",
            write: write_ospg_quad,
            decode: QuadEncoding::Ospg,
            order: &[2, 0, 1, 3],
            named: true,
        },
        KeyCase {
            name: "gspo",
            write: write_gspo_quad,
            decode: QuadEncoding::Gspo,
            order: &[3, 0, 1, 2],
            named: true,
        },
        KeyCase {
            name: "gpos",
            write: write_gpos_quad,
            decode: QuadEncoding::Gpos,
            order: &[3, 1, 2, 0],
            named: true,
        },
        KeyCase {
            name: "gosp",
            write: write_gosp_quad,
            decode: QuadEncoding::Gosp,
            order: &[3, 2, 0, 1],
            named: true,
        },
    ];
    for case in cases {
        let graph = if case.named {
            iri("urn:subject:7").into()
        } else {
            GraphName::DefaultGraph
        };
        let strict = Quad::new(
            BlankNode::new("foo").unwrap(),
            iri("urn:subject:7"),
            Literal::new_simple_literal("foo"),
            graph,
        );
        let encoded = EncodedQuad::from(GeneralizedQuadRef::from(strict.as_ref()));
        assert_eq!(encoded, EncodedQuad::from(strict.as_ref()));
        // bounded: one fixed key, with at most four 17-byte encoded terms.
        let mut actual = Vec::with_capacity(68);
        (case.write)(&mut actual, &encoded);
        let mut expected = Vec::with_capacity(68);
        for index in case.order {
            expected.extend_from_slice(&parts[*index]);
        }
        assert_eq!(actual, expected, "{} v1 bytes changed", case.name);
        assert_eq!(
            case.decode.decode(&actual).unwrap(),
            encoded,
            "{} v1 decode",
            case.name
        );
    }
}
