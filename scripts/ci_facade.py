"""Prepare one immutable, unpublishable facade cohort for provider CI."""

import argparse
import importlib.util
import json
import re
import shutil
import subprocess
import sys
import tomllib
from pathlib import Path
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def policy(path=None):
    document = json.loads((path or Path(__file__).with_name("ci-facade.json")).read_text())
    repositories = {
        "facade": "https://github.com/legra-ai/pyshacl-turbo",
        "declaration": "https://github.com/legra-ai/pyshacl-turbo",
        "owlrl": "https://github.com/legra-ai/owlrl-turbo",
    }
    if set(document) != set(repositories):
        raise ValueError("CI requires explicit facade, declaration and OWL source authorities")
    for name, repository in repositories.items():
        selected = document[name]
        if (
            set(selected) != {"repository", "revision"}
            or selected["repository"] != repository
            or not re.fullmatch(r"[0-9a-f]{40}", selected["revision"])
        ):
            raise ValueError("CI peers require their public repository and immutable revision")
    return document


def owner_manifest():
    return tomllib.loads((ROOT / "pyproject.toml").read_text())


def run(arguments, *, capture=False):
    """Peer validation runs before native imports and also supports Windows."""
    result = subprocess.run(
        list(map(str, arguments)),
        check=True,
        text=True,
        stdout=subprocess.PIPE if capture else None,
    )
    return result.stdout if capture else None


def owner_input():
    builder = load("build_facade_candidate")
    revision = run(["git", "-C", ROOT, "rev-parse", "HEAD"], capture=True).strip()
    return builder.freeze(ROOT, revision, "rdflib-turbo", run)


def checkout(selected, destination):
    if destination.exists():
        raise ValueError("An immutable peer checkout must have a fresh destination")
    run(["git", "clone", "--no-checkout", selected["repository"], destination])
    run(["git", "-C", destination, "checkout", "--detach", selected["revision"]])
    actual = run(["git", "-C", destination, "rev-parse", "HEAD"], capture=True).strip()
    if actual != selected["revision"]:
        raise ValueError("Immutable peer checkout differs from its source authority")


def retain_evidence(source, output, record, owlrl):
    rules = load("facade_candidate")
    # bounded: four original archives/wheels, declaration authority and three receipts.
    names = {
        *(
            record[name]
            for name in ("base_archive", "base_wheel", "candidate_archive", "candidate_wheel")
        ),
        record["declaration"]["archive"],
        "candidate-inputs.json",
        "dependency-binding.patch",
        "declaration-requirements.lock",
        "owlrl-inputs.json",
        owlrl["archive"],
        owlrl["wheel"],
    }
    originals = {name: rules.artifact_file(source, name) for name in names}
    output.mkdir(parents=True, exist_ok=False)
    for name, original in originals.items():
        target = output / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(original, target)


def prepare(output):
    output = output.resolve()
    if output.is_relative_to(ROOT) or not output.name.startswith("candidate-facade"):
        raise ValueError("Candidate evidence must have its own namespace outside provider source")
    selected = policy()
    owner = owner_input()
    facade = output.with_name(output.name + "-source")
    owlrl = output.with_name(output.name + "-owlrl-source")
    declaration = output.with_name(output.name + "-declaration-source")
    builder = load("build_facade_candidate")
    peers = (
        ("facade", "pyshacl-turbo", facade),
        ("declaration", "pyshacl-turbo", declaration),
        ("owlrl", "owlrl-turbo", owlrl),
    )
    frozen = {}
    for name, project, source in peers:
        checkout(selected[name], source)
        frozen[name] = builder.freeze(source, selected[name]["revision"], project, run)
    build_output = output.with_name(output.name + "-build")
    arguments = SimpleNamespace(
        facade_source=facade,
        facade_revision=selected["facade"]["revision"],
        provider_revision=owner["revision"],
        declaration_source=declaration,
        declaration_revision=selected["declaration"]["revision"],
        declaration_archive=None,
        declaration_sha256=None,
        output=build_output,
        python=sys.executable,
    )
    record = builder.build(arguments)
    owlrl_record = load("owl_companion_inputs").prepare(
        owlrl, selected["owlrl"], frozen["owlrl"], build_output, sys.executable, run
    )
    if owner_input() != owner:
        raise ValueError("Native provider source changed during candidate preparation")
    for name, project, source in peers:
        if builder.freeze(source, selected[name]["revision"], project, run) != frozen[name]:
            raise ValueError("Immutable peer source changed during candidate preparation")
    retain_evidence(build_output, output, record, owlrl_record)
    return output


def extract_original(archive, destination, sha256):
    if not destination.exists():
        load("build_facade_candidate").extract_candidate(archive, destination)
    load("coverage_archive").freeze(archive, destination, sha256, project="pyshacl-turbo")


def declaration_peer(manifest):
    selected = manifest["tool"]["uv"]["sources"]["pyshacl-turbo"]
    if selected != {"path": "../pyshacl-turbo", "editable": True}:
        raise ValueError("Provider CI requires its explicit adjacent facade declaration")
    return ROOT.parent / "pyshacl-turbo"


def verify_declaration(record, selected):
    authority = record.get("authority", {})
    if authority.get("kind") != "git" or authority.get("revision") != selected["revision"]:
        raise ValueError("Candidate declaration authority differs from independent CI policy")


def consume(bundle):
    bundle = bundle.resolve(strict=True)
    selected, owner, manifest = policy(), owner_input(), owner_manifest()
    verified = load("facade_candidate").verify_artifacts(
        bundle / "candidate-inputs.json",
        load("native_coverage").facade_payload,
        selected["facade"]["revision"],
        owner["revision"],
        manifest["project"]["version"],
    )
    record, artifacts = verified["record"], verified["artifacts"]
    if record["provider_input"] != owner:
        raise ValueError("Candidate receipt differs from the complete native provider source")
    verify_declaration(record["declaration"], selected["declaration"])
    builder = load("build_facade_candidate")
    facade_source = bundle / "candidate-source"
    extract_original(
        artifacts["candidate_archive"], facade_source, record["candidate_archive_sha256"]
    )
    # Original declaration bytes satisfy locked exports; the separately verified
    # candidate wheel is the sole installed Python facade in acceptance runs.
    peer = declaration_peer(manifest)
    declaration = record["declaration"]
    original = load("facade_candidate").artifact_file(bundle, declaration["archive"])
    extract_original(original, peer, declaration["archive_sha256"])
    owl_inputs = load("owl_companion_inputs")
    owl = owl_inputs.verify(bundle, selected["owlrl"], owl_inputs.locked_version(ROOT))
    environment = {
        "SHACL_WHEEL": str(artifacts["candidate_wheel"]),
        "SHACL_SOURCE": str(facade_source),
        "SHACL_CANDIDATE_RECEIPT": str(bundle / "candidate-inputs.json"),
        "SHACL_BASE_REVISION": selected["facade"]["revision"],
        "OWLRL_WHEEL": str(owl["wheel"]),
        "OWLRL_SOURCE": str(owl["source"]),
        "OWLRL_ARCHIVE": str(owl["archive"]),
        "OWLRL_SHA256": owl["record"]["archive_sha256"],
    }
    if builder.freeze(ROOT, owner["revision"], "rdflib-turbo", run) != owner:
        raise ValueError("Native provider source changed while arranging CI acceptance")
    return environment


def write_environment(destination, values):
    if any("\n" in str(value) or "\r" in str(value) for value in values.values()):
        raise ValueError("GitHub environment values must fit one line")
    if any(not re.fullmatch(r"[A-Z_][A-Z0-9_]*", name) for name in values):
        raise ValueError("Invalid CI environment variable name")
    with destination.open("a") as stream:
        for name, value in values.items():
            stream.write(f"{name}={value}\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    prepare_parser = commands.add_parser("prepare")
    prepare_parser.add_argument("--output", type=Path, required=True)
    consume_parser = commands.add_parser("consume")
    consume_parser.add_argument("--input", type=Path, required=True)
    consume_parser.add_argument("--environment-file", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "prepare":
        print(prepare(args.output))
    else:
        write_environment(args.environment_file, consume(args.input))


if __name__ == "__main__":
    main()
