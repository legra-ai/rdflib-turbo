use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
    dependencies: Dependencies,
}

#[derive(Deserialize)]
struct Dependencies {
    #[serde(rename = "pyshacl-turbo-engine")]
    engine: GitDependency,
}

#[derive(Deserialize)]
struct GitDependency {
    #[serde(rename = "git")]
    _git: String,
    rev: String,
}

pub fn from_manifest(source: &str) -> Result<String, String> {
    let manifest: Manifest =
        toml::from_str(source).map_err(|error| format!("Invalid native engine source: {error}"))?;
    let revision = manifest.dependencies.engine.rev;
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("Native engine source requires a complete immutable Git revision".to_owned());
    }
    Ok(revision)
}
