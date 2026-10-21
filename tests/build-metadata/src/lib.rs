#[cfg(test)]
#[path = "../../../build_support/engine_revision.rs"]
mod engine_revision;

#[cfg(test)]
mod tests {
    use super::engine_revision::from_manifest;

    fn manifest(revision: &str) -> String {
        format!(
            "[dependencies.pyshacl-turbo-engine]\ngit='https://example.test/engine'\nrev='{revision}'"
        )
    }

    #[test]
    fn source_revision_tracks_only_the_authoritative_native_dependency() {
        for revision in ["a".repeat(40), "b".repeat(40)] {
            assert_eq!(from_manifest(&manifest(&revision)).unwrap(), revision);
        }
        let source = format!(
            "{}\n[package.metadata]\nreported_revision='{}'",
            manifest(&"a".repeat(40)),
            "b".repeat(40)
        );
        assert_eq!(from_manifest(&source).unwrap(), "a".repeat(40));
    }

    #[test]
    fn missing_mutable_or_malformed_source_identity_is_a_hard_error() {
        for source in [
            "not TOML".to_owned(),
            "[dependencies]\npyshacl-turbo-engine='0.5'".to_owned(),
            "[dependencies.pyshacl-turbo-engine]\ngit='https://example.test/engine'".to_owned(),
            manifest("main"),
            manifest(&"a".repeat(39)),
            manifest(&"z".repeat(40)),
        ] {
            assert!(from_manifest(&source).is_err(), "accepted {source}");
        }
    }
}
