"""Run with -I in the actual isolated wheel environment, before and after timings."""

import hashlib
import importlib
import json
import sys
from importlib.metadata import version
from importlib.util import find_spec
from pathlib import Path
from zipfile import ZipFile


def attest(receipt):
    if sys.version.split()[0] != "3.13.16" or version("rdflib") != "7.6.0":
        raise ValueError("Wrong comparison Python or RDFLib version")
    for name in ("pyoxigraph", "pyshacl_turbo", "pyshacl", "owlrl"):
        if find_spec(name) is not None:
            raise ValueError(f"Unexpected optional/reference runtime: {name}")
    proof = {}
    native_library = None
    for name, key in [("rdflib_turbo", "wheel"), ("owlrl_turbo", "owlrl_wheel")]:
        wheel = Path(receipt[key])

        def sha(data):
            return hashlib.sha256(data).hexdigest()

        if sha(wheel.read_bytes()) != receipt[key + "_sha256"]:
            raise ValueError("Original wheel bytes changed")
        module = importlib.import_module(name)
        root = Path(module.__file__).resolve().parent
        if not root.is_relative_to(Path(sys.prefix).resolve()):
            raise ValueError("Loaded source outside isolated runtime")
        with ZipFile(wheel) as archive:
            expected = {
                n: sha(archive.read(n))
                for n in archive.namelist()
                if n.startswith(name + "/") and not n.endswith("/")
            }
        actual = {
            name + "/" + p.relative_to(root).as_posix(): sha(p.read_bytes())
            for p in root.rglob("*")
            if p.is_file() and "__pycache__" not in p.parts
        }
        if actual != expected:
            raise ValueError("Installed payload differs from original wheel")
        libraries = [n for n in actual if n.endswith((".so", ".pyd", ".dylib", ".dll"))]
        if len(libraries) != (1 if name == "rdflib_turbo" else 0):
            raise ValueError("Unexpected native payload")
        proof[name] = actual
        if name == "rdflib_turbo":
            native_library = root.parent / libraries[0]
    source_path = Path(__file__).with_name("comparison_identity.py")
    spec = importlib.util.spec_from_file_location("attested_comparison_source", source_path)
    source = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(source)
    derived = source.verify_receipt(receipt)
    native = importlib.import_module("rdflib_turbo._native")
    if Path(native.__file__).resolve() != native_library:
        raise ValueError("Loaded native library differs from the verified original wheel location")
    expected_library = proof["rdflib_turbo"]["rdflib_turbo/" + native_library.name]
    if hashlib.sha256(native_library.read_bytes()).hexdigest() != expected_library:
        raise ValueError("Loaded native library changed after its original payload check")
    if (
        native.SHACL_ENGINE_SOURCE != derived["engine"]["engine_source"]
        or native.SHACL_ENGINE_FINGERPRINT != derived["engine"]["engine_fingerprint"]
        or native.shacl.SHACL_ENGINE_FINGERPRINT != derived["engine"]["engine_fingerprint"]
    ):
        raise ValueError("Compiled source identity differs from independently derived inputs")
    proof.update(derived)
    return proof


if __name__ == "__main__":
    print(json.dumps(attest(json.loads(Path(sys.argv[1]).read_text())), sort_keys=True))
