"""Inference reads the native transaction, including pending writes."""

import pytest
from rdflib_turbo import Dataset, Store

PREFIX = """
@prefix : <urn:> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .
"""
DATA = (
    PREFIX
    + """
:Child rdfs:subClassOf :Parent . :a a :Child ; :p :b .
:p owl:inverseOf :inverse . :chain owl:propertyChainAxiom (:p :q) . :b :q :c .
:Person owl:hasKey (:id :code) .
:a a :Person ; :id "01" ; :code "x" .
:b a :Person ; :id "01" ; :code "x" .
:c a :Person ; :id "1" ; :code "x" .
"""
)


@pytest.mark.parametrize("mode", ["rdfs", "owlrl", "both"])
def test_store_inference_matches_dataset(mode):
    expected = Dataset().parse(data=DATA).infer(inference=mode)
    with Store() as store:
        assert store.parse(data=DATA).infer(inference=mode) is store
        # Compare graphs independently of parser-generated blank-node IDs.
        from rdflib.compare import isomorphic

        actual = store.query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }").graph
        reference = expected.query("CONSTRUCT { ?s ?p ?o } WHERE { ?s ?p ?o }").graph
        assert isomorphic(actual, reference)


def test_inference_sees_pending_writes_and_aborts_with_scope():
    with Store() as store:
        with pytest.raises(ValueError, match="abort"):
            with store.transaction() as transaction:
                transaction.parse(data=DATA)
                assert transaction.infer(inference="both") is transaction
                assert transaction.query("ASK { <urn:a> a <urn:Parent> }").askAnswer
                assert transaction.query("ASK { <urn:a> <urn:chain> <urn:c> }").askAnswer
                assert len(store) == 0
                raise ValueError("abort")
        assert len(store) == 0
        with store.transaction() as transaction:
            transaction.parse(data=DATA).infer(inference="both")
        assert store.query("ASK { <urn:b> <urn:inverse> <urn:a> }").askAnswer


@pytest.mark.parametrize("destination", [None, "urn:inferred"])
def test_union_inference_streams_graphs_and_writes_only_new_facts(destination):
    with Store() as store:
        store.parse(
            data="""<urn:a> a <urn:Child> <urn:data> .
<urn:Child> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:Parent> <urn:schema> .
""".replace(" a ", " <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> "),
            format="nquads",
        )
        store.infer(inference="rdfs", destination=destination)
        pattern = "<urn:a> a <urn:Parent>"
        if destination:
            pattern = f"GRAPH <{destination}> {{ {pattern} }}"
        assert store.query(f"ASK {{ {pattern} }}").askAnswer
        graph = "" if destination is None else f"GRAPH <{destination}>"
        assert not store.query(f"ASK {{ {graph} {{ <urn:a> a <urn:Child> }} }}").askAnswer
        # RDFS initializes resources from the terms present at each invocation.
        # Preserve that existing compatibility behavior on repeated calls too.
        expected = Dataset().parse(data=store.serialize(), format="nquads")
        expected.infer(inference="rdfs", destination=destination)
        store.infer(inference="rdfs", destination=destination)
        assert set(store.serialize().splitlines()) == set(expected.serialize().splitlines())


@pytest.mark.parametrize("mode,destination", [("invalid", None), ("rdfs", "not an iri")])
def test_invalid_inference_aborts_even_when_caught(mode, destination):
    with Store() as store:
        with pytest.raises(RuntimeError, match="closed"):
            with store.transaction() as transaction:
                transaction.parse(data=DATA)
                with pytest.raises((ValueError, NotImplementedError)):
                    transaction.infer(inference=mode, destination=destination)
        assert len(store) == 0


def test_malformed_list_aborts_earlier_inference_and_original_mutations():
    with Store() as store:
        store.parse(data=PREFIX + ":kept :p :value .")
        with pytest.raises(ValueError, match="list"):
            with store.transaction() as transaction:
                transaction.update("DELETE WHERE { ?s ?p ?o }")
                transaction.parse(data=PREFIX + ":p owl:propertyChainAxiom (:q) .")
                transaction.infer(inference="owlrl")
                # A list inferred/inserted after an earlier successful round also aborts it all.
                transaction.parse(data=PREFIX + ":broken owl:propertyChainAxiom :missing .")
                transaction.infer(inference="owlrl")
        assert len(store) == 1
        assert store.query("ASK { <urn:kept> <urn:p> <urn:value> }").askAnswer
        store.infer(inference="rdfs")


def test_persistent_inference_reopens_with_exact_literals(tmp_path):
    data = (
        PREFIX
        + ':p rdfs:subPropertyOf :q . :a :p "01"^^<http://www.w3.org/2001/XMLSchema#integer> .'
    )
    with Store(tmp_path / "db") as store:
        store.parse(data=data).infer(inference="rdfs")
    with Store(tmp_path / "db") as store:
        result = store.query("SELECT (STR(?o) AS ?lexical) WHERE { <urn:a> <urn:q> ?o }")
        assert str(next(iter(result)).lexical) == "01"


@pytest.mark.parametrize("destination", [None, "urn:inferred"])
def test_union_lists_treat_duplicate_edges_as_one_and_keep_exact_keys(destination):
    data = (
        PREFIX
        + """
:g1 {
    :chain owl:propertyChainAxiom :head .
    :head rdf:first :p ; rdf:rest :tail .
    :tail rdf:first :q ; rdf:rest rdf:nil .
    _:Class owl:hasKey :keys . :keys rdf:first :id ; rdf:rest rdf:nil .
    :a a _:Class ; :id "01" ; :p :b . :b :q :c .
}
:g2 {
    :head rdf:first :p ; rdf:rest :tail .
    :tail rdf:first :q ; rdf:rest rdf:nil .
    :keys rdf:first :id ; rdf:rest rdf:nil .
    :b a _:Class ; :id "01" . :c a _:Class ; :id "1" .
}
"""
    )
    with Store() as store:
        store.parse(data=data, format="trig").infer(inference="owlrl", destination=destination)
        graph = "" if destination is None else f"GRAPH <{destination}>"
        assert store.query(f"ASK {{ {graph} {{ <urn:a> <urn:chain> <urn:c> }} }}").askAnswer
        same = "<http://www.w3.org/2002/07/owl#sameAs>"
        assert store.query(f"ASK {{ {graph} {{ <urn:a> {same} <urn:b> }} }}").askAnswer
        assert not store.query(f"ASK {{ {graph} {{ <urn:a> {same} <urn:c> }} }}").askAnswer
