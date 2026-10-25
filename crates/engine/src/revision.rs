//! Source identity for the provider-owned engine and its semantic dependencies.
use sha2::{Digest, Sha256};

pub const SOURCES: &[(&str, &[u8])] = &[
    ("compile.rs", include_bytes!("compile.rs")),
    ("components.rs", include_bytes!("components.rs")),
    ("constraint_list.rs", include_bytes!("constraint_list.rs")),
    (
        "constraint_list_duration.rs",
        include_bytes!("constraint_list_duration.rs"),
    ),
    (
        "constraint_list_numbers.rs",
        include_bytes!("constraint_list_numbers.rs"),
    ),
    (
        "constraint_list_truthiness.rs",
        include_bytes!("constraint_list_truthiness.rs"),
    ),
    ("constraints.rs", include_bytes!("constraints.rs")),
    ("display.rs", include_bytes!("display.rs")),
    ("execute.rs", include_bytes!("execute.rs")),
    ("execute/backend.rs", include_bytes!("execute/backend.rs")),
    (
        "execute/constraint_advanced.rs",
        include_bytes!("execute/constraint_advanced.rs"),
    ),
    (
        "execute/constraint_list.rs",
        include_bytes!("execute/constraint_list.rs"),
    ),
    (
        "execute/constraint_relational.rs",
        include_bytes!("execute/constraint_relational.rs"),
    ),
    (
        "execute/constraint_scalar.rs",
        include_bytes!("execute/constraint_scalar.rs"),
    ),
    (
        "execute/constraints.rs",
        include_bytes!("execute/constraints.rs"),
    ),
    (
        "execute/data_list.rs",
        include_bytes!("execute/data_list.rs"),
    ),
    (
        "execute/evaluation.rs",
        include_bytes!("execute/evaluation.rs"),
    ),
    ("execute/report.rs", include_bytes!("execute/report.rs")),
    ("execute/rules.rs", include_bytes!("execute/rules.rs")),
    ("execute/targets.rs", include_bytes!("execute/targets.rs")),
    (
        "execute/validation.rs",
        include_bytes!("execute/validation.rs"),
    ),
    ("expression.rs", include_bytes!("expression.rs")),
    ("functions.rs", include_bytes!("functions.rs")),
    ("functions/query.rs", include_bytes!("functions/query.rs")),
    ("graph.rs", include_bytes!("graph.rs")),
    ("graph_read.rs", include_bytes!("graph_read.rs")),
    (
        "graph_read_generalized.rs",
        include_bytes!("graph_read_generalized.rs"),
    ),
    ("graph_views.rs", include_bytes!("graph_views.rs")),
    ("graph_write.rs", include_bytes!("graph_write.rs")),
    ("inference.rs", include_bytes!("inference.rs")),
    ("inference/axioms.rs", include_bytes!("inference/axioms.rs")),
    ("lib.rs", include_bytes!("lib.rs")),
    ("model.rs", include_bytes!("model.rs")),
    ("ontology.rs", include_bytes!("ontology.rs")),
    ("path.rs", include_bytes!("path.rs")),
    ("pattern.rs", include_bytes!("pattern.rs")),
    ("query_graph.rs", include_bytes!("query_graph.rs")),
    ("rdf11_store.rs", include_bytes!("rdf11_store.rs")),
    ("regex_policy.rs", include_bytes!("regex_policy.rs")),
    ("revision.rs", include_bytes!("revision.rs")),
    ("rule_delta.rs", include_bytes!("rule_delta.rs")),
    ("shapes_policy.rs", include_bytes!("shapes_policy.rs")),
    ("shapes_source.rs", include_bytes!("shapes_source.rs")),
    ("sparql.rs", include_bytes!("sparql.rs")),
    ("sparql_regex.rs", include_bytes!("sparql_regex.rs")),
    ("structural.rs", include_bytes!("structural.rs")),
    ("value.rs", include_bytes!("value.rs")),
    ("value/comparison.rs", include_bytes!("value/comparison.rs")),
];

fn update_source(digest: &mut Sha256, source: &[u8]) {
    for line in source.split_inclusive(|byte| *byte == b'\n') {
        if let Some(prefix) = line.strip_suffix(b"\r\n") {
            digest.update(prefix);
            digest.update(b"\n");
        } else {
            digest.update(line);
        }
    }
}

pub fn fingerprint() -> String {
    let mut digest = Sha256::new();
    update_source(&mut digest, include_bytes!("../Cargo.toml"));
    for (name, source) in SOURCES {
        digest.update(name.as_bytes());
        digest.update([0]);
        update_source(&mut digest, source);
        digest.update([0]);
    }
    for (name, identity) in [
        ("oxigraph", oxigraph::implementation_fingerprint()),
        ("oxrdf", oxrdf::implementation_fingerprint()),
        ("oxsdatatypes", oxsdatatypes::implementation_fingerprint()),
        ("spargebra", spargebra::implementation_fingerprint()),
        ("sparopt", sparopt::implementation_fingerprint()),
        ("spareval", spareval::implementation_fingerprint()),
        ("fancy-regex", fancy_regex::implementation_fingerprint()),
    ] {
        digest.update(name.as_bytes());
        digest.update([0]);
        digest.update(identity.as_bytes());
        digest.update([0]);
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn transport_line_endings_do_not_change_source_identity() {
        use sha2::{Digest, Sha256};
        let mut unix = Sha256::new();
        let mut windows = Sha256::new();
        super::update_source(&mut unix, b"first\nsecond\nlast\r");
        super::update_source(&mut windows, b"first\r\nsecond\r\nlast\r");
        assert_eq!(unix.finalize(), windows.finalize());
        let mut bare = Sha256::new();
        let mut empty = Sha256::new();
        super::update_source(&mut bare, b"\r");
        super::update_source(&mut empty, b"");
        assert_ne!(bare.finalize(), empty.finalize());
    }

    #[test]
    fn fingerprint_covers_every_engine_implementation_file() {
        fn visit(root: &std::path::Path, directory: &std::path::Path, names: &mut Vec<String>) {
            for entry in std::fs::read_dir(directory).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    visit(root, &path, names);
                } else if path.extension().is_some_and(|value| value == "rs")
                    && !path
                        .file_name()
                        .unwrap()
                        .to_str()
                        .unwrap()
                        .ends_with("_tests.rs")
                {
                    names.push(
                        path.strip_prefix(root)
                            .unwrap()
                            .to_str()
                            .unwrap()
                            .replace('\\', "/"),
                    );
                }
            }
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut names = Vec::new();
        visit(&root, &root, &mut names);
        names.sort();
        assert_eq!(
            names,
            super::SOURCES
                .iter()
                .map(|(name, _)| name.to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(super::fingerprint().len(), 64);
    }
}
