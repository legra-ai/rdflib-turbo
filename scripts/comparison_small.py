"""One cold installed-artifact inference and full bounded graph oracle."""

import argparse
import hashlib
import json
import resource
import sys
import time
from importlib.util import find_spec
from pathlib import Path

import owlrl_turbo
from rdflib import Graph as ReferenceGraph
from rdflib.compare import isomorphic
from rdflib_turbo import Graph

p = argparse.ArgumentParser()
p.add_argument("--case", required=True)
p.add_argument("--mode", default="both", choices=["rdfs", "owlrl", "both"])
p.add_argument("--fixtures", type=Path, required=True)
a = p.parse_args()
for name in ["pyoxigraph", "pyshacl_turbo", "pyshacl"]:
    assert find_spec(name) is None
entries = json.loads(a.fixtures.read_text())["entries"]
(e,) = [e for e in entries if e["case"] == a.case and e["mode"] == a.mode]
text = Path(e["input"]).read_text()
assert hashlib.sha256(text.encode()).hexdigest() == e["input_sha256"]
start = time.process_time_ns()
native = Graph().parse(data=text, format="nt")
parse_ns = time.process_time_ns() - start
start = time.process_time_ns()
wall = time.perf_counter_ns()
assert owlrl_turbo.expand(native, inference=a.mode, inplace=True) is native
cpu = time.process_time_ns() - start
wall = time.perf_counter_ns() - wall
peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss * (
    1 if sys.platform == "darwin" else 1024
)
print(
    json.dumps(
        {
            "stage": "inferred",
            "case": a.case,
            "parse_cpu_ns": parse_ns,
            "inference_cpu_ns": cpu,
            "inference_wall_ns": wall,
            "peak_before_export_bytes": peak,
            "output_count": len(native),
            "input_count": e["input_count"],
            "input_sha256": e["input_sha256"],
        }
    ),
    flush=True,
)
expected = Path(e["expected"])
assert hashlib.sha256(expected.read_bytes()).hexdigest() == e["expected_sha256"]
reference = ReferenceGraph().parse(data=expected.read_text(), format="nt")
actual = ReferenceGraph().parse(data=native.serialize(format="nt"), format="nt")
assert len(reference) == e["expected_count"] and isomorphic(actual, reference)
print(
    json.dumps({"stage": "audited", "count": len(actual), "full_small_graph_isomorphic": True}),
    flush=True,
)
native.close()
