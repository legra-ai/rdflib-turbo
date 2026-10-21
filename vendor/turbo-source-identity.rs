//! Build-time identity of the dependency Cargo is actually compiling.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn inputs(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    fn visit(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                visit(root, &path, files)?;
            } else if path.extension().is_some_and(|value| value == "rs") {
                files.push(path.strip_prefix(root).expect("crate source").to_owned());
            }
        }
        Ok(())
    }
    let mut files = vec![PathBuf::from("Cargo.toml"), PathBuf::from("build.rs")];
    visit(root, &root.join("src"), &mut files)?;
    files.sort();
    Ok(files)
}

fn bytes(hash: &mut Sha256, source: &[u8]) {
    for line in source.split_inclusive(|byte| *byte == b'\n') {
        if let Some(prefix) = line.strip_suffix(b"\r\n") {
            hash.update(prefix);
            hash.update(b"\n");
        } else {
            hash.update(line);
        }
    }
    hash.update([0]);
}

fn hex(hash: Sha256) -> String {
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn backend_capability(crate_name: &str, feature: &str) -> bool {
    // Only persistence implementation selection is independent of the borrowed
    // GraphRead query/RDF contract. New features are semantic by default.
    crate_name == "oxigraph"
        && matches!(
            feature,
            "CARGO_FEATURE_ROCKSDB"
                | "CARGO_FEATURE_ROCKSDB_DEBUG"
                | "CARGO_FEATURE_ROCKSDB_PKG_CONFIG"
        )
}

pub fn build() {
    let root = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let crate_name = std::env::var("CARGO_PKG_NAME").expect("package name");
    let files = inputs(&root).expect("enumerate dependency source identity");
    let mut semantic = Sha256::new();
    for path in &files {
        let name = path.to_str().expect("UTF-8 source path").replace('\\', "/");
        bytes(&mut semantic, name.as_bytes());
        bytes(&mut semantic, &std::fs::read(root.join(path)).expect("read identity input"));
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let helper = root.join("../turbo-source-identity.rs");
    bytes(&mut semantic, b"@shared/turbo-source-identity.rs");
    bytes(&mut semantic, &std::fs::read(&helper).expect("read identity helper"));
    println!("cargo:rerun-if-changed={}", helper.display());
    println!("cargo:rerun-if-changed=src");
    let mut features = std::env::vars()
        .filter(|(name, _)| name.starts_with("CARGO_FEATURE_"))
        .collect::<Vec<_>>();
    features.sort();
    let mut backend = Sha256::new();
    bytes(&mut backend, crate_name.as_bytes());
    for (name, value) in features {
        println!("cargo:rerun-if-env-changed={name}");
        let identity = format!("{name}={value}");
        if backend_capability(&crate_name, &name) {
            bytes(&mut backend, identity.as_bytes());
        } else {
            bytes(&mut semantic, identity.as_bytes());
        }
    }
    println!("cargo:rustc-env=TURBO_IMPLEMENTATION_FINGERPRINT={}", hex(semantic));
    println!("cargo:rustc-env=TURBO_BACKEND_CAPABILITY_FINGERPRINT={}", hex(backend));
}
