"""Build an unpublishable dependency-bound facade from immutable source inputs."""

import argparse
import hashlib
import importlib.util
import json
import re
import shutil
import tarfile
import tomllib
import zipfile
from pathlib import Path, PurePosixPath


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def digest(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def require_output(path, sources):
    path = path.resolve()
    if not path.name.startswith("candidate-facade"):
        raise ValueError("Test inputs require the candidate-facade namespace")
    if any(path.is_relative_to(source.resolve()) for source in sources):
        raise ValueError("Candidate outputs must be outside frozen source checkouts")


def artifact_reference(root, path):
    root, path = root.resolve(), path.resolve()
    if not path.is_relative_to(root):
        raise ValueError("Input points outside candidate artifact namespace")
    return path.relative_to(root).as_posix()


def peer_name(manifest, package):
    binding = manifest.get("tool", {}).get("uv", {}).get("sources", {}).get(package)
    # Directory naming only: registry/Git declarations are never replaced here.
    if binding is None or "path" not in binding:
        return package
    path = PurePosixPath(binding["path"])
    if (
        binding.get("editable") is not True
        or len(path.parts) != 2
        or path.parts[0] != ".."
        or not re.fullmatch(re.escape(package) + r"(?:-[a-z0-9-]+)?", path.parts[1])
    ):
        raise ValueError("Expected an explicit adjacent package peer")
    return path.parts[1]


def freeze(source, revision, project, run):
    if not re.fullmatch(r"[0-9a-f]{40}", revision):
        raise ValueError("Expected an immutable candidate source revision")
    if run(["git", "-C", source, "rev-parse", "HEAD"], capture=True).strip() != revision:
        raise ValueError("Candidate source does not match its immutable revision")
    if run(["git", "-C", source, "status", "--porcelain", "--untracked-files=all"], capture=True):
        raise ValueError("Candidate construction requires clean original source inputs")
    manifest = tomllib.loads((source / "pyproject.toml").read_text())
    if manifest["project"]["name"] != project:
        raise ValueError("Wrong candidate source project identity")
    files = {}
    for name in filter(
        None, run(["git", "-C", source, "ls-files", "-z"], capture=True).split("\0")
    ):
        path = source / name
        if not path.resolve(strict=True).is_relative_to(source):
            raise ValueError("Candidate source escapes its immutable checkout")
        files[name] = digest(path)
    return {"revision": revision, "files": files}


def clone(source, revision, destination, run):
    run(["git", "clone", "--no-checkout", source, destination])
    run(["git", "-C", destination, "checkout", "--detach", revision])


def build_pair(source, output, python, run):
    output.mkdir()
    run(["uv", "build", "--sdist", "--python", python, "--out-dir", output, source])
    archives = list(output.glob("*.tar.gz"))
    if len(archives) != 1:
        raise ValueError("Expected one original facade source archive")
    run(["uv", "build", "--wheel", "--python", python, "--out-dir", output, archives[0]])
    wheels = list(output.glob("*.whl"))
    if len(wheels) != 1:
        raise ValueError("Expected one original universal facade wheel")
    return archives[0], wheels[0]


def wheel_metadata(wheel):
    with zipfile.ZipFile(wheel) as archive:
        names = [name for name in archive.namelist() if name.endswith(".dist-info/METADATA")]
        if len(names) != 1:
            raise ValueError("Expected one original facade wheel metadata record")
        return archive.read(names[0])


def extract_candidate(archive, destination):
    destination.mkdir()
    rules = load("coverage_archive")
    with tarfile.open(archive, "r:gz") as package:
        for member in package:
            if member.isdir():
                continue
            _, relative = rules.member_path(member.name)
            if not member.isfile():
                raise ValueError("Candidate archive contains a non-file member")
            target = destination.joinpath(*relative.parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            with package.extractfile(member) as source, target.open("xb") as output:
                for chunk in iter(lambda: source.read(65536), b""):
                    output.write(chunk)


def build(arguments):
    owner = Path(__file__).resolve().parents[1]
    facade = arguments.facade_source.resolve(strict=True)
    output = arguments.output.resolve()
    require_output(output, [owner, facade])
    coverage, rules, declarations = (
        load("native_coverage"),
        load("facade_candidate"),
        load("facade_declaration"),
    )
    run = coverage.run
    provider_input = freeze(owner, arguments.provider_revision, "rdflib-turbo", run)
    facade_input = freeze(facade, arguments.facade_revision, "pyshacl-turbo", run)
    selected = declarations.select(
        owner,
        facade,
        facade_input,
        lambda source, revision: freeze(source, revision, "pyshacl-turbo", run),
        source=arguments.declaration_source,
        revision=arguments.declaration_revision,
        archive=arguments.declaration_archive,
        sha256=arguments.declaration_sha256,
    )
    require_output(output, [owner, facade, selected["source"]])
    original = tomllib.loads((facade / "pyproject.toml").read_text())
    provider = tomllib.loads((owner / "pyproject.toml").read_text())
    if original["build-system"]["build-backend"] != "hatchling.build":
        raise ValueError("Candidate source must already be a pure facade")
    version = rules.candidate_version(
        original["project"]["version"], arguments.facade_revision, arguments.provider_revision
    )
    provider_version = provider["project"]["version"]
    uv = run(["uv", "--version"], capture=True).strip()
    if not uv.startswith("uv 0.13.1 "):
        raise ValueError("Candidate artifacts require the pinned uv 0.13.1 builder")
    output.mkdir(parents=True, exist_ok=False)
    cohort = output / "cohort"
    cohort.mkdir()
    native_peer = cohort / peer_name(original, "rdflib-turbo")
    base_peer = cohort / "canonical-facade"
    declaration_peer = cohort / peer_name(provider, "pyshacl-turbo")
    declared_manifest = tomllib.loads((selected["source"] / "pyproject.toml").read_text())
    if peer_name(declared_manifest, "rdflib-turbo") != native_peer.name:
        raise ValueError("Declaration peer names a different native source role")
    clone(owner, arguments.provider_revision, native_peer, run)
    clone(facade, arguments.facade_revision, base_peer, run)
    if selected["authority"]["kind"] == "git":
        clone(selected["source"], selected["authority"]["revision"], declaration_peer, run)
    else:
        extract_candidate(arguments.declaration_archive, declaration_peer)
    exported = output / "declaration-requirements.lock"
    run(
        [
            "uv",
            "export",
            "--project",
            native_peer,
            "--python",
            arguments.python,
            "--locked",
            "--group",
            "coverage",
            "--no-dev",
            "--no-emit-project",
            "--no-emit-package",
            "pyshacl-turbo",
            "--no-hashes",
            "--output-file",
            exported,
        ]
    )
    if freeze(native_peer, arguments.provider_revision, "rdflib-turbo", run) != provider_input:
        raise ValueError("Original owner lock changed during declaration export")
    base_archive, base_wheel = build_pair(base_peer, output / "originals", arguments.python, run)
    if selected["authority"]["kind"] == "original-sdist":
        declaration_originals = output / "declaration-originals"
        declaration_originals.mkdir()
        declaration_archive = declaration_originals / arguments.declaration_archive.name
        shutil.copyfile(arguments.declaration_archive, declaration_archive)
    elif selected["authority"]["revision"] == arguments.facade_revision:
        declaration_archive = base_archive
    else:
        declaration_originals = output / "declaration-originals"
        declaration_originals.mkdir()
        run(
            [
                "uv",
                "build",
                "--sdist",
                "--python",
                arguments.python,
                "--out-dir",
                declaration_originals,
                declaration_peer,
            ]
        )
        (declaration_archive,) = declaration_originals.glob("*.tar.gz")
    declaration_record = {
        "source": artifact_reference(output, declaration_peer),
        "version": declarations.locked_version(owner),
        "authority": selected["authority"],
        "archive": artifact_reference(output, declaration_archive),
        "archive_sha256": digest(declaration_archive),
    }
    declarations.verify_receipt(declaration_record, declaration_archive, rules.archive_record)
    candidate = cohort / "candidate-worktree"
    clone(facade, arguments.facade_revision, candidate, run)
    binding = "../" + native_peer.name
    uv_project = ["uv", "--directory", candidate]
    common = ["--no-sync", "--python", arguments.python]
    run([*uv_project, "add", "--editable", binding, *common])
    run([*uv_project, "add", f"rdflib-turbo=={provider_version}", *common])
    if any(
        rules.is_provider(value)
        for value in original["project"].get("optional-dependencies", {}).get("test", [])
    ):
        run(
            [*uv_project, "add", "--optional", "test", f"rdflib-turbo=={provider_version}", *common]
        )
    run([*uv_project, "version", version, *common])
    changed = set(run(["git", "-C", candidate, "diff", "--name-only"], capture=True).splitlines())
    if changed != {"pyproject.toml", "uv.lock"}:
        raise ValueError("Candidate modified files outside dependency metadata")
    rules.verify_manifest(
        original,
        tomllib.loads((candidate / "pyproject.toml").read_text()),
        provider_version,
        version,
        binding,
    )
    rules.verify_lock(
        tomllib.loads((facade / "uv.lock").read_text()),
        tomllib.loads((candidate / "uv.lock").read_text()),
    )
    patch = run(["git", "-C", candidate, "diff", "--binary"], capture=True)
    (output / "dependency-binding.patch").write_text(patch)
    archive, wheel = build_pair(candidate, output / "candidate-artifacts", arguments.python, run)
    archive_proof = rules.verify_archives(
        base_archive,
        digest(base_archive),
        archive,
        digest(archive),
        provider_version,
        version,
        binding,
    )
    base_files, _ = rules.archive_record(base_archive, digest(base_archive))
    if any(
        facade_input["files"][name] != sha
        for name, sha in base_files.items()
        if name in facade_input["files"]
    ):
        raise ValueError("Canonical source archive differs from its immutable Git input")
    if coverage.facade_payload(base_wheel) != coverage.facade_payload(wheel):
        raise ValueError("Candidate wheel changed canonical facade program bytes")
    rules.verify_pkginfo(
        wheel_metadata(base_wheel), wheel_metadata(wheel), provider_version, version
    )
    candidate_source = cohort / "candidate-source"
    extract_candidate(archive, candidate_source)
    if freeze(owner, arguments.provider_revision, "rdflib-turbo", run) != provider_input:
        raise ValueError("Original provider source changed during candidate construction")
    if freeze(facade, arguments.facade_revision, "pyshacl-turbo", run) != facade_input:
        raise ValueError("Original facade source changed during candidate construction")
    declaration_after = declarations.select(
        owner,
        facade,
        facade_input,
        lambda source, revision: freeze(source, revision, "pyshacl-turbo", run),
        source=arguments.declaration_source,
        revision=arguments.declaration_revision,
        archive=arguments.declaration_archive,
        sha256=arguments.declaration_sha256,
    )
    if declaration_after != selected:
        raise ValueError("Original declaration source changed during candidate construction")
    result = {
        "schema": 1,
        "kind": "test-only-facade-candidate",
        "publishable": False,
        "artifact_namespace": "candidate-facade",
        "uv": uv,
        "python": arguments.python,
        "base_source": artifact_reference(output, base_peer),
        "base_input": facade_input,
        "provider_source": artifact_reference(output, native_peer),
        "provider_input": provider_input,
        "declaration": declaration_record,
        "locked_export_sha256": digest(exported),
        "provider_version": provider_version,
        "candidate_version": version,
        "binding": binding,
        "base_archive": artifact_reference(output, base_archive),
        "base_archive_sha256": digest(base_archive),
        "base_wheel": artifact_reference(output, base_wheel),
        "base_wheel_sha256": digest(base_wheel),
        "candidate_archive": artifact_reference(output, archive),
        "candidate_archive_sha256": digest(archive),
        "candidate_wheel": artifact_reference(output, wheel),
        "candidate_wheel_sha256": digest(wheel),
        "candidate_source": artifact_reference(output, candidate_source),
        "archive_proof": archive_proof,
        "dependency_patch_sha256": digest(output / "dependency-binding.patch"),
    }
    (output / "candidate-inputs.json").write_text(
        json.dumps(result, indent=2, sort_keys=True) + "\n"
    )
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--facade-source", type=Path, required=True)
    parser.add_argument("--facade-revision", required=True)
    parser.add_argument("--provider-revision", required=True)
    parser.add_argument("--declaration-source", type=Path)
    parser.add_argument("--declaration-revision")
    parser.add_argument("--declaration-archive", type=Path)
    parser.add_argument("--declaration-sha256")
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--python", default="3.13")
    result = build(parser.parse_args())
    print(
        json.dumps(
            {
                key: value
                for key, value in result.items()
                if key not in {"base_input", "provider_input", "archive_proof", "declaration"}
            }
        )
    )


if __name__ == "__main__":
    main()
