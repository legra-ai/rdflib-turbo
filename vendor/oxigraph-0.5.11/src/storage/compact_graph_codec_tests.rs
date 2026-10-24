//! Graph-only storage identity remains compact without changing persisted bytes.
use super::binary_encoder::*;
use super::numeric_encoder::{EncodedQuad, EncodedTerm, StrHash};
use super::small_string::SmallString;

type Writer = fn(&mut Vec<u8>, &EncodedQuad);

fn named(id: u8) -> EncodedTerm {
    EncodedTerm::NamedNode {
        iri_id: StrHash::from_be_bytes([id; 16]),
    }
}

fn graph_cases() -> Vec<(EncodedTerm, Vec<u8>)> {
    // bounded: all five existing graph identity forms plus full-width ID controls.
    let mut named_bytes = vec![1];
    named_bytes.extend([0xa5; 16]);
    let mut big_bytes = vec![10];
    big_bytes.extend([0x5a; 16]);
    let mut result = vec![
        (named(0xa5), named_bytes),
        (
            EncodedTerm::BigBlankNode {
                id_id: StrHash::from_be_bytes([0x5a; 16]),
            },
            big_bytes,
        ),
    ];
    for id in [[0; 16], [0xff; 16]] {
        let mut bytes = vec![8];
        bytes.extend(id);
        result.push((EncodedTerm::NumericalBlankNode { id }, bytes));
    }
    for text in ["foo", "abcdefghijklmno", "éééééééa"] {
        let value = SmallString::from_utf8(text.as_bytes()).unwrap();
        let mut bytes = vec![9];
        bytes.extend_from_slice(text.as_bytes());
        bytes.resize(16, 0);
        bytes.push(u8::try_from(text.len()).unwrap());
        result.push((EncodedTerm::SmallBlankNode(value), bytes));
    }
    result
}

fn quad(graph: EncodedTerm) -> EncodedQuad {
    EncodedQuad {
        subject: named(1),
        predicate: named(2),
        object: named(3),
        graph_name: graph.try_into().unwrap(),
    }
}

fn term_bytes(id: u8) -> Vec<u8> {
    let mut result = vec![1];
    result.extend([id; 16]);
    result
}

fn named_orders() -> [(QuadEncoding, Writer, [usize; 4]); 6] {
    [
        (QuadEncoding::Spog, write_spog_quad, [0, 1, 2, 3]),
        (QuadEncoding::Posg, write_posg_quad, [1, 2, 0, 3]),
        (QuadEncoding::Ospg, write_ospg_quad, [2, 0, 1, 3]),
        (QuadEncoding::Gspo, write_gspo_quad, [3, 0, 1, 2]),
        (QuadEncoding::Gpos, write_gpos_quad, [3, 1, 2, 0]),
        (QuadEncoding::Gosp, write_gosp_quad, [3, 2, 0, 1]),
    ]
}

#[test]
fn compact_graph_names_preserve_every_v1_named_index_key_byte() {
    for (graph, bytes) in graph_cases() {
        let quad = quad(graph);
        let fields = [term_bytes(1), term_bytes(2), term_bytes(3), bytes];
        for (encoding, write, order) in named_orders() {
            let expected: Vec<_> = order
                .into_iter()
                .flat_map(|slot| fields[slot].clone())
                .collect();
            let mut actual = Vec::new();
            write(&mut actual, &quad);
            assert_eq!(actual, expected);
            assert_eq!(encoding.decode(&actual).unwrap(), quad);
        }
    }
}

#[test]
fn compact_graph_names_preserve_every_v1_default_index_key_byte() {
    let quad = quad(EncodedTerm::DefaultGraph);
    let fields = [term_bytes(1), term_bytes(2), term_bytes(3)];
    let cases: [(QuadEncoding, Writer, [usize; 3]); 3] = [
        (QuadEncoding::Dspo, write_spo_quad, [0, 1, 2]),
        (QuadEncoding::Dpos, write_pos_quad, [1, 2, 0]),
        (QuadEncoding::Dosp, write_osp_quad, [2, 0, 1]),
    ];
    for (encoding, write, order) in cases {
        let expected: Vec<_> = order
            .into_iter()
            .flat_map(|slot| fields[slot].clone())
            .collect();
        let mut actual = Vec::new();
        write(&mut actual, &quad);
        assert_eq!(actual, expected);
        assert_eq!(encoding.decode(&actual).unwrap(), quad);
    }
}

#[test]
fn compact_graph_name_decoding_rejects_literal_graph_slots_in_all_indexes() {
    // Frozen ordinary string-literal encoding, valid as an object but never a
    // graph name. Generalized subject/predicate support does not widen graph names.
    let mut literal = vec![16, b'x'];
    literal.resize(16, 0);
    literal.push(1);
    let fields = [term_bytes(1), term_bytes(2), term_bytes(3), literal];
    for (encoding, _, order) in named_orders() {
        let bytes: Vec<_> = order
            .into_iter()
            .flat_map(|slot| fields[slot].clone())
            .collect();
        assert!(encoding.decode(&bytes).is_err());
    }
}
