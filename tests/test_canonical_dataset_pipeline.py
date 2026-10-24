"""Complete three-product Dataset pipeline, preserving every physical context."""

import owlrl
import owlrl_turbo
import pyshacl
import pyshacl_turbo
import pytest
from rdflib import BNode, Graph, URIRef
from rdflib import Dataset as RDFLibDataset
from rdflib.compare import to_canonical_graph
from rdflib_turbo import Dataset, StoreDataset


def run_pipeline(factory, reasoning, validation):
    source = factory(default_union=False)
    source.parse(
        data="<urn:Child> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:Parent>.",
        format="turtle",
    )
    named = source.graph(URIRef("urn:source:named"))
    blank = source.graph(BNode("source-blank"))
    source.graph(URIRef("urn:source:empty"))
    for index in range(8):
        graph = named if index % 2 else blank
        subject = URIRef(f"urn:subject:{index}")
        graph.add(
            (
                subject,
                URIRef("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
                URIRef("urn:Child"),
            )
        )
        graph.add((subject, URIRef("urn:required"), URIRef("urn:present")))
    source.bind("example", URIRef("urn:example:"))
    shapes = Graph().parse(
        data="""
@prefix sh: <http://www.w3.org/ns/shacl#>.
<urn:Shape> a sh:NodeShape; sh:targetClass <urn:Parent>;
 sh:property <urn:Property>; sh:rule <urn:Rule>.
<urn:Property> sh:path <urn:required>; sh:minCount 1.
<urn:Rule> a sh:TripleRule; sh:subject sh:this;
 sh:predicate <urn:checked>; sh:object true.
""",
        format="turtle",
    )

    def observation():
        return {
            "quads": {tuple(term.n3() for term in quad) for quad in source.quads((None,) * 4)},
            "graph_domain": {graph.identifier.n3() for graph in source.graphs()},
            "default_union": source.default_union,
            "namespaces": dict(source.namespaces()),
        }

    before = observation()
    reasoning.DeductiveClosure(reasoning.RDFS_OWLRL_Semantics).expand(source)
    inferred = observation()
    rules_return = validation.shacl_rules(source, shacl_graph=shapes, advanced=True, inplace=True)
    # The original returns a live RdfLibDataGraph wrapper; native inputs may
    # return themselves. The shared owner and complete observable state matter.
    assert rules_return.store is source.store
    result = validation.validate(source, shacl_graph=shapes)
    assert result[0] is True, result[2]
    output = {
        "before": before,
        "inference": inferred,
        "rules": observation(),
        "report": {tuple(term.n3() for term in triple) for triple in to_canonical_graph(result[1])},
        "text": result[2],
    }
    # Hold reference graphs/reports alive while comparing the independent run.
    return output, (source, shapes, rules_return, result[1])


@pytest.mark.parametrize("factory", (Dataset, StoreDataset))
def test_canonical_dataset_pipeline_matches_all_original_contexts(factory, monkeypatch):
    expected, expected_owners = run_pipeline(RDFLibDataset, owlrl, pyshacl)
    assert len(expected["before"]["quads"]) == 17
    assert len(expected["inference"]["quads"]) == 396
    assert len(expected["rules"]["quads"]) == 404

    def forbid_conversion(*args, **kwargs):
        raise AssertionError("Shared native Dataset pipeline must not copy or serialize input")

    monkeypatch.setattr(Dataset, "from_rdflib", forbid_conversion)
    monkeypatch.setattr(Dataset, "to_rdflib", forbid_conversion)
    monkeypatch.setattr(factory, "copy", forbid_conversion)
    monkeypatch.setattr(factory, "serialize", forbid_conversion)
    actual, actual_owners = run_pipeline(factory, owlrl_turbo, pyshacl_turbo)
    assert actual == expected
    assert expected_owners[0] is not actual_owners[0]
