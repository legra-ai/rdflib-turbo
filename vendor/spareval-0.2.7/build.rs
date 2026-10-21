//! Compact identity of the evaluator implementation actually compiled by Cargo.
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn inputs(root: &Path) -> std::io::Result<Vec<PathBuf>> {
    fn visit(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for entry in std::fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                visit(root, &path, files)?;
            } else if path.extension().is_some_and(|value| value == "rs") {
                files.push(
                    path.strip_prefix(root)
                        .expect("source is inside crate")
                        .to_owned(),
                );
            }
        }
        Ok(())
    }
    let mut files = vec![PathBuf::from("Cargo.toml"), PathBuf::from("build.rs")];
    visit(root, &root.join("src"), &mut files)?;
    files.sort();
    Ok(files)
}

fn fingerprint(root: &Path, files: &[PathBuf], features: &[String]) -> std::io::Result<String> {
    let mut hash = Sha256::new();
    for path in files {
        let name = path
            .to_str()
            .expect("source path is UTF-8")
            .replace('\\', "/");
        hash.update(name.as_bytes());
        hash.update([0]);
        let bytes = std::fs::read(root.join(path))?;
        for line in bytes.split_inclusive(|byte| *byte == b'\n') {
            if let Some(prefix) = line.strip_suffix(b"\r\n") {
                hash.update(prefix);
                hash.update(b"\n");
            } else {
                hash.update(line);
            }
        }
        hash.update([0]);
    }
    for feature in features {
        hash.update(feature.as_bytes());
        hash.update([0]);
    }
    Ok(hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(not(test))]
fn main() {
    let root =
        PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"));
    let files = inputs(&root).expect("enumerate evaluator identity inputs");
    let mut features = std::env::vars()
        .filter_map(|(name, value)| {
            name.strip_prefix("CARGO_FEATURE_")
                .map(|_| format!("{name}={value}"))
        })
        .collect::<Vec<_>>();
    features.sort();
    let identity = fingerprint(&root, &files, &features).expect("read evaluator identity inputs");
    println!("cargo:rerun-if-changed=src");
    for file in &files {
        println!("cargo:rerun-if-changed={}", file.display());
    }
    for feature in &features {
        let (name, _) = feature.split_once('=').expect("feature selection");
        println!("cargo:rerun-if-env-changed={name}");
    }
    println!("cargo:rustc-env=SPAREVAL_IMPLEMENTATION_FINGERPRINT={identity}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_discovers_every_source_and_hashes_contents_and_features() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let files = inputs(root).unwrap();
        assert!(files.contains(&PathBuf::from("src/expression.rs")));
        assert!(files.contains(&PathBuf::from("build.rs")));
        assert!(files.contains(&PathBuf::from("Cargo.toml")));
        assert!(files.windows(2).all(|pair| pair[0] < pair[1]));
        let plain = fingerprint(root, &files, &[]).unwrap();
        let feature = fingerprint(
            root,
            &files,
            &["CARGO_FEATURE_EXACT_CUSTOM_FUNCTION_TERMS=1".into()],
        )
        .unwrap();
        assert_eq!(plain.len(), 64);
        assert_ne!(plain, feature);
        let mut omitted = files;
        omitted.retain(|path| path != &PathBuf::from("src/expression.rs"));
        assert_ne!(plain, fingerprint(root, &omitted, &[]).unwrap());
    }
}
