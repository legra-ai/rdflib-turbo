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
    native = importlib.import_module("rdflib_turbo._native")
    expected = receipt["expected_identity"]
    if (
        native.SHACL_ENGINE_REVISION != expected["engine_revision"]
        or native.SHACL_ENGINE_FINGERPRINT != expected["engine_fingerprint"]
    ):
        raise ValueError("Compiled source identity differs from independently derived inputs")
    proof["engine"] = expected
    return proof


if __name__ == "__main__":
    print(json.dumps(attest(json.loads(Path(sys.argv[1]).read_text())), sort_keys=True))
