"""Per-call axiom options match the pinned reference's complete RDF relation."""

from itertools import product

import pytest
from inference_observers import union_graph
from owlrl import DeductiveClosure, OWLRL_Semantics, RDFS_OWLRL_Semantics, RDFS_Semantics
from rdflib import RDF, RDFS, URIRef
from rdflib import Dataset as ReferenceDataset
from rdflib import Graph as ReferenceGraph
from rdflib_turbo import Dataset, Store

CLASSES = {"rdfs": RDFS_Semantics, "owlrl": OWLRL_Semantics, "both": RDFS_OWLRL_Semantics}
DOCUMENT = """
<urn:options:s> <urn:options:p> "7"^^<http://www.w3.org/2001/XMLSchema#integer> ;
    <http://www.w3.org/1999/02/22-rdf-syntax-ns#_1> <urn:options:o> ;
    <http://www.w3.org/1999/02/22-rdf-syntax-ns#_3> <urn:options:o> .
"""


def ordinary(graph):
    return set(graph)


def native_graph(graph):
    return union_graph(graph)


@pytest.fixture(
    scope="module", params=list(product(CLASSES, [False, True], [False, True], ["", DOCUMENT]))
)
def reference_case(request):
    mode, axioms, datatypes, document = request.param
    reference = ReferenceGraph().parse(data=document, format="turtle")
    DeductiveClosure(CLASSES[mode], axiomatic_triples=axioms, datatype_axioms=datatypes).expand(
        reference
    )
    return mode, axioms, datatypes, document, ordinary(reference)


@pytest.mark.parametrize("api", ["dataset", "store", "transaction"])
def test_option_combinations_match_reference(reference_case, api):
    mode, axioms, datatypes, document, expected = reference_case
    options = {"inference": mode, "axiomatic_triples": axioms, "datatype_axioms": datatypes}
    if api == "dataset":
        graph = Dataset().parse(data=document)
        assert graph.infer(**options) is graph
        assert ordinary(native_graph(graph)) == expected
        return
    with Store() as store:
        if api == "transaction":
            with store.transaction() as graph:
                graph.parse(data=document).infer(**options)
                assert ordinary(native_graph(graph)) == expected
        else:
            store.parse(data=document).infer(**options)
        assert ordinary(native_graph(store)) == expected


@pytest.mark.parametrize("mode", CLASSES)
def test_explicit_false_flags_preserve_default_behavior(mode):
    source = Dataset().parse(data=DOCUMENT)
    default = source.copy().infer(inference=mode)
    explicit = source.copy().infer(inference=mode, axiomatic_triples=False, datatype_axioms=False)
    assert ordinary(native_graph(explicit)) == ordinary(native_graph(default))


def quad_set(graph):
    return {(s, p, o, context) for s, p, o, context in graph.quads()}


@pytest.mark.parametrize(
    "mode, axioms, datatypes", list(product(CLASSES, [False, True], [False, True]))
)
def test_destination_axioms_and_delta_match_reference(mode, axioms, datatypes):
    source = ReferenceDataset()
    # An axiom already present in another context is still explicitly inserted
    # into the requested destination when the reference enables that table.
    source.graph(URIRef("urn:source")).add((RDF.type, RDF.type, RDF.Property))
    source.graph(URIRef("urn:source")).add((URIRef("urn:s"), RDF["_1"], URIRef("urn:o")))
    source.graph(URIRef("urn:other")).add((URIRef("urn:s"), RDF["_1"], URIRef("urn:o")))
    source.graph(URIRef("urn:source")).add((URIRef("urn:t"), RDF["_1"], URIRef("urn:o")))
    original = quad_set(source)
    native = Dataset.from_rdflib(source)
    options = {
        "inference": mode,
        "destination": "urn:out",
        "axiomatic_triples": axioms,
        "datatype_axioms": datatypes,
    }
    delta = native.infer_delta(**options)
    DeductiveClosure(CLASSES[mode], axiomatic_triples=axioms, datatype_axioms=datatypes).expand(
        source, destination=URIRef("urn:out")
    )
    assert quad_set(native.to_rdflib()) == quad_set(source)
    assert quad_set(delta.to_rdflib()) == quad_set(source) - original


@pytest.mark.parametrize("persistent", [False, True])
def test_container_count_uses_distinct_union_triples_and_reference_repetition(persistent, tmp_path):
    path = tmp_path / "store" if persistent else None
    with Store(path) as store:
        store.parse(
            data="""
        <urn:a> { <urn:s> <http://www.w3.org/1999/02/22-rdf-syntax-ns#_1> <urn:o> . }
        <urn:b> { <urn:s> <http://www.w3.org/1999/02/22-rdf-syntax-ns#_1> <urn:o> . }
        """,
            format="trig",
        )
        store.infer(inference="rdfs", axiomatic_triples=True)
        assert not store.query(
            f"ASK {{ <{RDF['_2']}> a <{RDFS.ContainerMembershipProperty}> }}", default_union=True
        )
        store.update(f"INSERT DATA {{ <urn:t> <{RDF['_1']}> <urn:o> }}")
        store.infer(inference="rdfs", axiomatic_triples=True)
        assert store.query(
            f"ASK {{ <{RDF['_2']}> a <{RDFS.ContainerMembershipProperty}> }}", default_union=True
        )


@pytest.mark.parametrize("api", ["dataset", "delta", "store", "transaction"])
def test_optional_axioms_roll_back_with_late_inference_failure(api):
    malformed = """
    @prefix owl: <http://www.w3.org/2002/07/owl#> .
    @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
    <urn:alias> rdfs:subPropertyOf owl:hasKey .
    <urn:Class> <urn:alias> _:missingList .
    _:missingList <http://www.w3.org/1999/02/22-rdf-syntax-ns#first> <urn:key>;
      <http://www.w3.org/1999/02/22-rdf-syntax-ns#rest> _:missingList .
    """
    options = {"axiomatic_triples": True, "datatype_axioms": True}
    if api in ("dataset", "delta"):
        graph = Dataset().parse(data=malformed)
        before = graph.serialize()
        operation = graph.infer if api == "dataset" else graph.infer_delta
        with pytest.raises(ValueError, match="recursive rdf:rest"):
            operation(**options)
        assert graph.serialize() == before
        return
    with Store() as store:
        if api == "store":
            store.parse(data=malformed)
            before = store.serialize()
            with pytest.raises(ValueError, match="recursive rdf:rest"):
                store.infer(**options)
            assert store.serialize() == before
        else:
            with pytest.raises(RuntimeError, match=r"closed|failed"):
                with store.transaction() as graph:
                    graph.parse(data=malformed)
                    with pytest.raises(ValueError, match="recursive rdf:rest"):
                        graph.infer(**options)
            assert len(store) == 0
