use serde::Deserialize;

#[derive(Deserialize)]
struct Manifest {
    dependencies: Dependencies,
}

#[derive(Deserialize)]
struct Dependencies {
    #[serde(rename = "pyshacl-turbo-engine")]
    engine: LocalDependency,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalDependency {
    path: String,
}

pub fn from_manifest(source: &str) -> Result<String, String> {
    let manifest: Manifest =
        toml::from_str(source).map_err(|error| format!("Invalid native engine source: {error}"))?;
    let path = manifest.dependencies.engine.path;
    if path != "crates/engine" {
        return Err("Native engine source must be owned at crates/engine".to_owned());
    }
    Ok(path)
}
