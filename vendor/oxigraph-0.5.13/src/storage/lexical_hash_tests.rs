use super::numeric_encoder::StrHash;

#[test]
fn lexical_hash_vectors_are_stable() {
    // bounded: four fixed algorithm vectors, independent of stored RDF data.
    let vectors = [
        ("", 0xb91c_5ed0_31fb_3303_2134_935d_61d9_b40d_u128),
        ("foo", 0xc212_4ffe_f7c6_2b98_7f1a_22d0_84b2_f283_u128),
        (
            "urn:subject:7",
            0x99ae_f647_b115_002f_47fe_b41e_d9d8_31c5_u128,
        ),
        ("é漢😀", 0x7550_e848_0197_2283_54da_9005_12c9_4ac7_u128),
    ];
    for (text, expected) in vectors {
        let expected = expected.to_be_bytes();
        assert_eq!(StrHash::new(text).to_be_bytes(), expected, "{text:?}");
        assert_eq!(StrHash::from_be_bytes(expected).to_be_bytes(), expected);
    }
}
