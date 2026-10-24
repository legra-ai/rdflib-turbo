"""Public synthetic bounded schema/intersection/equality fixture, no external data."""

import argparse
import hashlib
import json
import resource
import sys
import time
from importlib.util import find_spec

p = argparse.ArgumentParser()
p.add_argument("--implementation", choices=["python", "turbo"], required=True)
p.add_argument("--input-count", type=int, choices=[25000, 100000], required=True)
a = p.parse_args()
assert find_spec("pyoxigraph") is None
if a.implementation == "turbo":
    import owlrl_turbo as reasoning
    from rdflib_turbo import Graph
else:
    import owlrl as reasoning
    from rdflib import Graph
RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
RDFS = "http://www.w3.org/2000/01/rdf-schema#"
OWL = "http://www.w3.org/2002/07/owl#"
# bounded: exactly the requested capped synthetic input facts; equality rings have at most8 members.
rows = []
for i in range(a.input_count * 32 // 100):
    rows.extend(
        [
            f"<urn:s:{i}> <{RDF}type> <urn:A> .\n",
            f"<urn:s:{i}> <{RDF}type> <urn:B> .\n",
            f"<urn:s:{i}> <urn:required> <urn:present> .\n",
        ]
    )
for left, right in [("A", "C"), ("C", "D"), ("D", "E"), ("E", "F")]:
    rows.append(f"<urn:{left}> <{RDFS}subClassOf> <urn:{right}> .\n")
rows.extend(
    [
        f"<urn:I> <{OWL}intersectionOf> <urn:list:1> .\n",
        f"<urn:list:1> <{RDF}first> <urn:A> .\n",
        f"<urn:list:1> <{RDF}rest> <urn:list:2> .\n",
        f"<urn:list:2> <{RDF}first> <urn:B> .\n",
        f"<urn:list:2> <{RDF}rest> <{RDF}nil> .\n",
        f"<urn:required> <{RDFS}subPropertyOf> <urn:related> .\n",
    ]
)
for i in range(a.input_count - len(rows)):
    target = i // 8 * 8 + (i + 1) % 8
    rows.append(f"<urn:s:{i}> <{OWL}sameAs> <urn:s:{target}> .\n")
assert len(rows) == a.input_count
text = "".join(rows)
input_sha = hashlib.sha256(text.encode()).hexdigest()
del rows
start = time.process_time_ns()
graph = Graph().parse(data=text, format="nt")
del text
parse_ns = time.process_time_ns() - start
print(
    json.dumps(
        {
            "stage": "parsed",
            "input_count": len(graph),
            "input_sha256": input_sha,
            "parse_cpu_ns": parse_ns,
            "implementation": a.implementation,
            "python": sys.version,
        }
    ),
    flush=True,
)
start = time.process_time_ns()
wall = time.perf_counter_ns()
if a.implementation == "turbo":
    assert reasoning.expand(graph, inference="both", inplace=True) is graph
else:
    reasoning.DeductiveClosure(reasoning.RDFS_OWLRL_Semantics).expand(graph)
ns = time.process_time_ns() - start
wall = time.perf_counter_ns() - wall
peak = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
if sys.platform != "darwin":
    peak *= 1024
print(
    json.dumps(
        {
            "stage": "inferred",
            "inference_cpu_ns": ns,
            "inference_wall_ns": wall,
            "peak_before_export_bytes": peak,
            "output_count": len(graph),
        }
    ),
    flush=True,
)
# Bounded streaming multiset witnesses; no sorted output/materialized result graph.
mod = 1 << 256
total = 0
squares = 0
count = 0
for triple in graph:
    h = int.from_bytes(
        hashlib.sha256(json.dumps([str(t) for t in triple], ensure_ascii=False).encode()).digest(),
        "big",
    )
    total = (total + h) % mod
    squares = (squares + h * h) % mod
    count += 1
print(
    json.dumps(
        {
            "stage": "audited",
            "count": count,
            "sha256_sum": hex(total),
            "sha256_square_sum": hex(squares),
            "witness_only": "order-independent streaming digest, not an exact stored graph comparison",
        }
    ),
    flush=True,
)
