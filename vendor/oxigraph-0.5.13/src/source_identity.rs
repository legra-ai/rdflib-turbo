/// SHA-256 of the actual compiled source and semantic feature selections.
pub fn implementation_fingerprint() -> &'static str {
    env!("TURBO_IMPLEMENTATION_FINGERPRINT")
}

/// SHA-256 of the separately audited persistence backend capability selections.
pub fn backend_capability_fingerprint() -> &'static str {
    env!("TURBO_BACKEND_CAPABILITY_FINGERPRINT")
}
