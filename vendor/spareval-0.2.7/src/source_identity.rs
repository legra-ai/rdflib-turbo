//! Compact source and feature identity of the evaluator actually compiled.

/// Build-time SHA-256 of this evaluator's implementation and Cargo features.
/// A downstream Cargo patch attests its own implementation, not another tree.
pub fn implementation_fingerprint() -> &'static str {
    env!("SPAREVAL_IMPLEMENTATION_FINGERPRINT")
}

#[cfg(test)]
#[path = "../build.rs"]
mod build_identity_tests;
