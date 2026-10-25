"""Small source graphs exercise proof boundaries without compiling a native artifact."""

import hashlib
import io
import subprocess
import tarfile

FORKS = ("oxigraph", "oxrdf", "oxsdatatypes", "spargebra", "sparopt", "spareval", "fancy-regex")
REGISTRY = "registry+https://github.com/rust-lang/crates.io-index"


def write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)


def make_owner(directory):
    source = directory / "base-source"
    write(
        source / "Cargo.toml",
        '[package]\nname="rdflib-turbo-native"\nversion="0.2.0-alpha.19"\n'
        '[dependencies]\npyshacl-turbo-engine={path="crates/engine"}\n[patch.crates-io]\n'
        + "".join(f'{name}={{path="vendor/{name}-0.1.0"}}\n' for name in FORKS),
    )
    engine = source / "crates/engine"
    write(
        engine / "Cargo.toml",
        '[package]\nname="pyshacl-turbo-engine"\nversion="0.7.0"\n'
        '[dependencies]\nshacl-structural-model="=6.2.0"\n',
    )
    write(engine / "src/lib.rs", "pub fn validate() -> bool { true }\n")
    write(
        engine / "src/revision.rs",
        'include_bytes!("lib.rs");\ninclude_bytes!("revision.rs");\nfn update_source() {}\n',
    )
    packages = [
        {
            "id": "engine",
            "name": "pyshacl-turbo-engine",
            "version": "0.7.0",
            "source": None,
            "manifest_path": str(engine / "Cargo.toml"),
        }
    ]
    nodes = [
        {
            "id": "engine",
            "features": [],
            "deps": [{"name": "shacl_structural_model", "pkg": "model"}],
        }
    ]
    write(source / "vendor/turbo-source-identity.rs", "// stable independent field framing\n")
    for name in FORKS:
        fork = source / "vendor" / (name + "-0.1.0")
        write(fork / "Cargo.toml", f'[package]\nname="{name}"\nversion="0.1.0"\n')
        write(fork / "build.rs", "fn main() {}\n")
        write(fork / "src/lib.rs", "pub fn evaluate() -> bool { true }\n")
        packages.append(
            {
                "id": name,
                "name": name,
                "version": "0.1.0",
                "source": None,
                "manifest_path": str(fork / "Cargo.toml"),
            }
        )
        nodes.append({"id": name, "features": ["std", "semantic"], "deps": []})
    model = directory / "cargo/registry/src/index/shacl-structural-model-6.2.0"
    write(model / "Cargo.toml", '[package]\nname="shacl-structural-model"\nversion="6.2.0"\n')
    write(model / "src/lib.rs", "pub struct TypedModel;\n")
    archive = directory / "cargo/registry/cache/index/shacl-structural-model-6.2.0.crate"
    archive.parent.mkdir(parents=True)
    with tarfile.open(archive, "w:gz") as bundle:
        for name in ("Cargo.toml", "src/lib.rs"):
            data = (model / name).read_bytes()
            entry = tarfile.TarInfo(model.name + "/" + name)
            entry.size = len(data)
            bundle.addfile(entry, io.BytesIO(data))
    checksum = hashlib.sha256(archive.read_bytes()).hexdigest()
    write(model / ".cargo-ok", "1")
    write(
        source / "Cargo.lock",
        'version=4\n[[package]]\nname="shacl-structural-model"\n'
        f'version="6.2.0"\nsource="{REGISTRY}"\nchecksum="{checksum}"\n',
    )
    packages.append(
        {
            "id": "model",
            "name": "shacl-structural-model",
            "version": "6.2.0",
            "source": REGISTRY,
            "manifest_path": str(model / "Cargo.toml"),
        }
    )
    nodes.append({"id": "model", "features": [], "deps": []})
    packages.append(
        {
            "id": "provider",
            "name": "rdflib-turbo-native",
            "version": "0.2.0-alpha.19",
            "source": None,
            "manifest_path": str(source / "Cargo.toml"),
        }
    )
    nodes.append(
        {
            "id": "provider",
            "features": [],
            "deps": [{"name": "pyshacl_turbo_engine", "pkg": "engine"}],
        }
    )
    return {
        "source": source,
        "metadata": {"packages": packages, "resolve": {"nodes": nodes}},
        "model": model,
        "model_archive": archive,
        "model_checksum": checksum,
    }


def commit_source(source):
    for command in (
        ["git", "init", "--initial-branch=main"],
        ["git", "config", "user.name", "Proof fixture"],
        ["git", "config", "user.email", "fixture@example.invalid"],
        ["git", "config", "commit.gpgsign", "false"],
        ["git", "add", "."],
        ["git", "commit", "-m", "test(fixture): retain source graph"],
    ):
        subprocess.run(command, cwd=source, check=True, capture_output=True)
    return subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=source, text=True).strip()
