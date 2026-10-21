#[path = "build_support/engine_revision.rs"]
mod engine_revision;

fn main() {
    println!("cargo::rerun-if-changed=Cargo.toml");
    println!("cargo::rerun-if-changed=build_support/engine_revision.rs");
    let revision = engine_revision::from_manifest(include_str!("Cargo.toml"))
        .expect("Native engine source identity must be immutable");
    println!("cargo::rustc-env=RDFLIB_TURBO_SHACL_ENGINE_REVISION={revision}");
}
