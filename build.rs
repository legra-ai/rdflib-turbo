//! Link scalar-only CPython functions using the interpreter selected by PyO3.

use std::{env, fs, path::PathBuf};

#[path = "build_support/engine_revision.rs"]
mod engine_revision;

fn main() {
    println!("cargo::rerun-if-changed=Cargo.toml");
    println!("cargo::rerun-if-changed=build_support/engine_revision.rs");
    let revision = engine_revision::from_manifest(include_str!("Cargo.toml"))
        .expect("Native engine source identity must be immutable");
    println!("cargo::rustc-env=RDFLIB_TURBO_SHACL_ENGINE_REVISION={revision}");

    let mut bindings = String::new();
    if env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        let library = pyo3_build_config::get()
            .lib_name()
            .expect("Windows CPython must have a DLL name");
        bindings.push_str(&format!(
            "#[link(name = {library:?}, kind = \"raw-dylib\")]\n"
        ));
    }
    bindings.push_str(
        "unsafe extern \"C\" {\n\
         fn _PyUnicode_IsAlpha(ch: u32) -> std::ffi::c_int;\n\
         fn _PyUnicode_IsNumeric(ch: u32) -> std::ffi::c_int;\n\
         fn _PyUnicode_IsWhitespace(ch: u32) -> std::ffi::c_int;\n\
         }\n",
    );
    let output = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo must supply OUT_DIR"));
    fs::write(output.join("python_scalar_imports.rs"), bindings)
        .expect("Cannot write CPython scalar imports");
    println!("cargo:rerun-if-changed=build.rs");
}
