"""SHACL draft maintenance reaches borrowed native graph validation."""

import pyshacl_turbo as turbo
import pytest
from rdflib import Graph as ReferenceGraph
from rdflib import Literal, URIRef
from rdflib.namespace import RDF, SH
from rdflib_turbo import Graph, StoreDataset


@pytest.mark.parametrize("dataset", [False, True])
@pytest.mark.parametrize("enabled", [False, True])
@pytest.mark.parametrize("separator", ["\n", "\v", "\f", "\r"])
def test_single_line_uses_the_native_owner_and_keeps_report_terms(
    monkeypatch, dataset, enabled, separator
):
    source = StoreDataset() if dataset else Graph(identifier=URIRef("urn:selected"))
    graph = source.default_graph if dataset else source
    focus, predicate = URIRef("urn:focus"), URIRef("urn:label")
    value = Literal(f"left{separator}right", lang="en")
    graph.add((focus, predicate, value))
    shapes = ReferenceGraph().parse(
        data=f"""@prefix sh: <http://www.w3.org/ns/shacl#>.
        <urn:shape> a sh:NodeShape; sh:targetNode <{focus}>;
          sh:property [ sh:path <{predicate}>; sh:singleLine {str(enabled).lower()} ].""",
        format="turtle",
    )

    def materialized(*args, **kwargs):
        pytest.fail("native SHACL validation must not serialize the input graph")

    monkeypatch.setattr(source, "serialize", materialized)
    conforms, report, _ = turbo.validate(source, shacl_graph=shapes)
    assert conforms is not enabled
    results = list(report.subjects(RDF.type, SH.ValidationResult))
    if enabled:
        assert len(results) == 1
        result = results[0]
        assert report.value(result, SH.value) == value
        assert report.value(result, SH.focusNode) == focus
        assert report.value(result, SH.resultPath) == predicate
        assert report.value(result, SH.sourceConstraintComponent) == URIRef(
            "http://www.w3.org/ns/shacl#SingleLineConstraintComponent"
        )
    else:
        assert not results
    assert (focus, predicate, value) in graph
