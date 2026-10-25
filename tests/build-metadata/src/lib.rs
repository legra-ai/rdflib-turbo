#[cfg(test)]
#[path = "../../../build_support/engine_source.rs"]
mod engine_source;

#[cfg(test)]
mod tests {
    use super::engine_source::from_manifest;

    #[test]
    fn provider_owned_engine_is_identified_by_its_local_manifest() {
        let source = "[dependencies.pyshacl-turbo-engine]\npath='crates/engine'";
        assert_eq!(from_manifest(source).unwrap(), "crates/engine");
    }

    fn manifest(path: &str) -> String {
        format!("[dependencies.pyshacl-turbo-engine]\npath='{path}'")
    }

    #[test]
    fn source_tracks_only_the_authoritative_local_dependency() {
        let source = format!(
            "{}\n[package.metadata]\nreported_source='../another-engine'",
            manifest("crates/engine")
        );
        assert_eq!(from_manifest(&source).unwrap(), "crates/engine");
    }

    #[test]
    fn external_ambiguous_or_malformed_source_identity_is_a_hard_error() {
        for source in [
            "not TOML".to_owned(),
            "[dependencies]\npyshacl-turbo-engine='0.5'".to_owned(),
            "[dependencies.pyshacl-turbo-engine]\ngit='https://example.test/engine'".to_owned(),
            manifest("../another-engine"),
            manifest("/engine"),
            manifest("crates/python"),
            format!("{}\nrev='{}'", manifest("crates/engine"), "a".repeat(40)),
        ] {
            assert!(from_manifest(&source).is_err(), "accepted {source}");
        }
    }
}
