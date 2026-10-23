"""Independent original OWL-RL full-graph goldens for bounded synthetic equality cases."""

import argparse
import hashlib
import json
from pathlib import Path

from owlrl import DeductiveClosure, OWLRL_Semantics, RDFS_OWLRL_Semantics, RDFS_Semantics
from rdflib import BNode, Graph, URIRef
from rdflib.compare import to_canonical_graph
from rdflib.namespace import OWL, RDF, RDFS

parser = argparse.ArgumentParser()
parser.add_argument("--output", type=Path, required=True)
args = parser.parse_args()
OUT = args.output
OUT.mkdir(exist_ok=False)


def n(i):
    return URIRef(f"urn:equality:n:{i}")


p = URIRef("urn:equality:property")
cases = {}
cases["isolated-selfs"] = [(n(i), OWL.sameAs, n(i)) for i in range(32)]
cases["closed-pairs"] = [
    (n(i + a), OWL.sameAs, n(i + b)) for i in range(0, 32, 2) for a in range(2) for b in range(2)
]
cases["closed-clique16"] = [(n(i), OWL.sameAs, n(j)) for i in range(16) for j in range(16)]
cases["sparse-chain16"] = [(n(i), OWL.sameAs, n(i + 1)) for i in range(15)]
cases["ring8"] = [(n(i), OWL.sameAs, n((i + 1) % 8)) for i in range(8)]
cases["half-circulant16"] = [
    (n(i), OWL.sameAs, n((i + j) % 16)) for i in range(16) for j in range(1, 9)
]
cases["fork-with-ordinary-facts"] = [
    (n(0), OWL.sameAs, n(1)),
    (n(0), OWL.sameAs, n(2)),
    (n(1), OWL.sameAs, n(3)),
    (n(0), p, n(4)),
]
cases["blank-terminal"] = [
    (n(0), OWL.sameAs, BNode("terminal")),
    (n(0), OWL.sameAs, n(2)),
    (n(0), p, n(4)),
]
cases["class-equivalence"] = [
    (n(0), OWL.equivalentClass, n(1)),
    (n(2), RDF.type, n(0)),
    (n(1), RDFS.subClassOf, n(3)),
]
cases["property-equivalence"] = [
    (p, OWL.sameAs, n(0)),
    (n(1), p, n(2)),
    (p, OWL.equivalentProperty, n(3)),
]
classes = {"rdfs": RDFS_Semantics, "owlrl": OWLRL_Semantics, "both": RDFS_OWLRL_Semantics}
entries = []
for label, triples in cases.items():
    initial = Graph()
    for triple in triples:
        initial.add(triple)
    data = initial.serialize(format="nt")
    input_path = OUT / (label + ".input.nt")
    input_path.write_text(data)
    for mode, semantics in classes.items():
        reference = Graph().parse(data=data, format="nt")
        DeductiveClosure(semantics).expand(reference)
        assert len(reference) < 10000
        # bounded: these fixtures have at most32 nodes and fewer than10000 closure facts.
        expected = "".join(
            sorted(to_canonical_graph(reference).serialize(format="nt").splitlines(keepends=True))
        )
        path = OUT / (label + "." + mode + ".expected.nt")
        path.write_text(expected)
        entries.append(
            {
                "case": label,
                "mode": mode,
                "input": str(input_path),
                "input_sha256": hashlib.sha256(input_path.read_bytes()).hexdigest(),
                "input_count": len(initial),
                "expected": str(path),
                "expected_sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                "expected_count": len(reference),
            }
        )
(OUT / "fixtures.json").write_text(
    json.dumps(
        {
            "oracle": "original owlrl7.6.2 with RDFLib7.6.0; complete canonical small RDF graphs",
            "entries": entries,
        },
        indent=2,
    )
    + "\n"
)
print(
    json.dumps(
        {
            "cases": len(cases),
            "mode_cases": len(entries),
            "maximum_facts": max(e["expected_count"] for e in entries),
        }
    )
)
