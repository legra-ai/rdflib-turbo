//! Bind the immutable engine source identity to the native provider.

#[path = "build_support/engine_source.rs"]
mod engine_source;

fn main() {
    println!("cargo::rerun-if-changed=Cargo.toml");
    println!("cargo::rerun-if-changed=build_support/engine_source.rs");
    let source = engine_source::from_manifest(include_str!("Cargo.toml"))
        .expect("Native engine source must belong to this provider");
    println!("cargo::rustc-env=RDFLIB_TURBO_SHACL_ENGINE_SOURCE={source}");
}
