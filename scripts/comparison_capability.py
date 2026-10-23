"""Require the actual public test companion's native Graph route before timings."""

import json

import owlrl_turbo
from rdflib import URIRef
from rdflib.namespace import RDF, RDFS
from rdflib_turbo import Graph

graph = Graph()
graph.add((URIRef("urn:x"), RDF.type, URIRef("urn:A")))
graph.add((URIRef("urn:A"), RDFS.subClassOf, URIRef("urn:B")))
if owlrl_turbo.NativeGraph is not Graph:
    raise ValueError("Companion and provider do not share the native Graph class")
if owlrl_turbo.expand(graph, inference="both", inplace=True) is not graph:
    raise ValueError("Companion replaced the native Graph during in-place expansion")
if (URIRef("urn:x"), RDF.type, URIRef("urn:B")) not in graph:
    raise ValueError("Actual installed native Graph inference did not derive the expected fact")
print(
    json.dumps(
        {
            "native_graph_class_identity": True,
            "inplace_owner_retained": True,
            "expected_native_inference": True,
            "output_count": len(graph),
        }
    )
)
graph.close()
