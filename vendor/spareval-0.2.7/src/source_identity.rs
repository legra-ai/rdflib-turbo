/// SHA-256 of the actual compiled source and semantic feature selections.
pub fn implementation_fingerprint() -> &'static str {
    env!("TURBO_IMPLEMENTATION_FINGERPRINT")
}
