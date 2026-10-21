import subprocess
import sys
from importlib.metadata import version

import pytest
from rdflib import BNode, Literal, URIRef
from rdflib import Dataset as ReferenceDataset
from rdflib.namespace import XSD
from rdflib_turbo import Dataset


def test_public_version_matches_installed_distribution():
    import rdflib_turbo

    assert rdflib_turbo.__version__ == version("rdflib-turbo")


@pytest.mark.parametrize("release", ["9.8.7", "1.2.3a4"])
def test_public_version_follows_independently_versioned_distribution(tmp_path, release):
    metadata = tmp_path / f"rdflib_turbo-{release}.dist-info"
    metadata.mkdir()
    (metadata / "METADATA").write_text(
        f"Metadata-Version: 2.1\nName: rdflib-turbo\nVersion: {release}\n", encoding="utf-8"
    )
    subprocess.run(
        [
            sys.executable,
            "-I",
            "-c",
            "import sys; sys.path.insert(0, sys.argv[1]); "
            "from importlib.metadata import version; import rdflib_turbo; "
            "assert version('rdflib-turbo') == sys.argv[2]; "
            "assert rdflib_turbo.__version__ == sys.argv[2], rdflib_turbo.__version__",
            str(tmp_path),
            release,
        ],
        cwd=tmp_path,
        check=True,
        capture_output=True,
        text=True,
    )


def test_exact_literals_and_select():
    dataset = Dataset().parse(
        data='<urn:s> <urn:p> "01"^^<http://www.w3.org/2001/XMLSchema#integer>, "1"^^<http://www.w3.org/2001/XMLSchema#integer>, "text", "text"^^<http://www.w3.org/2001/XMLSchema#string> .'
    )
    values = {row.o for row in dataset.query("SELECT ?o WHERE { <urn:s> <urn:p> ?o }")}
    assert len(dataset) == len(values) == 4
    assert Literal("01", datatype=XSD.integer, normalize=False) in values
    assert Literal("text") in values
    assert Literal("text", datatype=XSD.string) in values


def test_named_graphs_and_default_union():
    dataset = Dataset().parse(data="<urn:g> { <urn:s> <urn:p> <urn:o> }", format="trig")
    assert bool(dataset.query("ASK { GRAPH <urn:g> { <urn:s> <urn:p> <urn:o> } }"))
    assert not bool(dataset.query("ASK { <urn:s> <urn:p> <urn:o> }"))
    assert bool(dataset.query("ASK { <urn:s> <urn:p> <urn:o> }", default_union=True))


def test_parse_error_does_not_commit_partial_input():
    dataset = Dataset().parse(data="<urn:s> <urn:p> <urn:o> .")
    before = dataset.serialize()
    with pytest.raises(ValueError):
        dataset.parse(data="<urn:a> <urn:b> <urn:c> . invalid")
    assert dataset.serialize() == before


def test_document_blank_nodes_are_separate():
    dataset = Dataset()
    dataset.parse(data="_:a <urn:p> <urn:o> .")
    dataset.parse(data="_:a <urn:p> <urn:o> .")
    assert len(dataset) == 2


def test_rdflib_conversion_preserves_contexts_and_blank_nodes():
    source = ReferenceDataset()
    node = BNode("shared")
    source.default_graph.add(
        (node, URIRef("urn:p"), Literal("01", datatype=XSD.integer, normalize=False))
    )
    source.graph(URIRef("urn:g")).add((node, URIRef("urn:p"), Literal("hello", lang="en")))
    result = Dataset.from_rdflib(source).to_rdflib()
    assert set(source.quads()) == set(result.quads())


def test_round_trip_and_file_input(tmp_path):
    path = tmp_path / "data.trig"
    path.write_text('<urn:g> { <urn:s> <urn:p> "hello"@en }')
    dataset = Dataset().parse(path, format="trig")
    output = dataset.serialize(format="trig")
    assert set(Dataset().parse(data=output, format="trig").to_rdflib().quads()) == set(
        dataset.to_rdflib().quads()
    )


def test_construct_and_describe():
    dataset = Dataset().parse(data="<urn:s> <urn:p> <urn:o> .")
    result = dataset.query("CONSTRUCT { ?s <urn:q> ?o } WHERE { ?s <urn:p> ?o }")
    assert result.type == "CONSTRUCT"
    assert (URIRef("urn:s"), URIRef("urn:q"), URIRef("urn:o")) in result.graph
    assert len(dataset.query("DESCRIBE <urn:s>").graph) == 1


def test_query_errors_and_unsupported_formats():
    dataset = Dataset()
    with pytest.raises(ValueError):
        dataset.query("INVALID")
    with pytest.raises(NotImplementedError):
        dataset.parse(data="", format="unknown")
    with pytest.raises(TypeError):
        dataset.parse(data="", source="also-specified")


def test_copy_is_independent_and_pyshacl_accepts_conversion():
    dataset = Dataset().parse(data="<urn:s> <urn:p> <urn:o> .")
    copy = dataset.copy().parse(data="<urn:a> <urn:b> <urn:c> .")
    assert len(dataset) == 1
    assert len(copy) == 2


def test_unbound_variables_and_ordered_results():
    dataset = Dataset().parse(data="<urn:a> <urn:p> 1 . <urn:b> <urn:p> 2 .")
    result = dataset.query(
        "SELECT ?s ?missing WHERE { ?s <urn:p> ?o OPTIONAL { ?s <urn:q> ?missing } } ORDER BY ?s"
    )
    assert [tuple(row) for row in result] == [(URIRef("urn:a"), None), (URIRef("urn:b"), None)]


def test_rdflib_default_graph_stays_default_inside_native_runtime():
    source = ReferenceDataset()
    source.default_graph.add((URIRef("urn:s"), URIRef("urn:p"), URIRef("urn:o")))
    native = Dataset.from_rdflib(source)
    assert bool(native.query("ASK { <urn:s> <urn:p> <urn:o> }"))
    assert not bool(native.query("ASK { GRAPH ?g { <urn:s> <urn:p> <urn:o> } }"))


@pytest.mark.parametrize(
    "expression",
    [
        'CONCAT("a", "b")',
        'STRDT("ab", <http://www.w3.org/2001/XMLSchema#string>)',
        "<http://www.w3.org/2001/XMLSchema#string>(3)",
    ],
)
def test_computed_strings_match_rdflib_identity(expression):
    query = f"SELECT ({expression} AS ?value) WHERE {{}}"
    expected = next(iter(ReferenceDataset().query(query))).value
    assert next(iter(Dataset().query(query))).value == expected


def test_graph_only_serializer_rejects_named_graphs():
    dataset = Dataset().parse(data="<urn:g> { <urn:s> <urn:p> <urn:o> }", format="trig")
    with pytest.raises(ValueError):
        dataset.serialize(format="nt")


def test_relative_iris_and_blank_graph_names():
    dataset = Dataset().parse(
        data="_:g { <s> <p> <o> }", format="trig", publicID="https://example.org/"
    )
    assert bool(
        dataset.query(
            "ASK { GRAPH ?g { <https://example.org/s> <https://example.org/p> <https://example.org/o> } }"
        )
    )
    roundtrip = Dataset.from_rdflib(dataset.to_rdflib())
    assert set(roundtrip.to_rdflib().quads()) == set(dataset.to_rdflib().quads())


@pytest.mark.parametrize("mode", ["rdfs", "owlrl", "both"])
def test_native_inference(mode):
    source = Dataset().parse(
        data="<urn:C> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:D> . <urn:x> a <urn:C> ."
    )
    assert source.infer(inference=mode) is source
    assert bool(source.query("ASK { <urn:x> a <urn:D> }"))


def test_native_inference_error_is_atomic():
    source = Dataset().parse(data="<urn:C> <http://www.w3.org/2002/07/owl#hasKey> _:broken .")
    before = source.serialize()
    with pytest.raises(ValueError, match="Malformed RDF list"):
        source.infer()
    assert source.serialize() == before


def test_utf8_bytes_file_base_override_and_invalid_input(tmp_path):
    assert len(Dataset().parse(data=b'<urn:s> <urn:p> "hello" .')) == 1
    path = tmp_path / "relative.ttl"
    path.write_text("<s> <p> <o> .")
    dataset = Dataset().parse(path, publicID="https://example.org/")
    assert bool(
        dataset.query(
            "ASK { <https://example.org/s> <https://example.org/p> <https://example.org/o> }"
        )
    )
    with pytest.raises(TypeError):
        Dataset().parse(data=object())
    with pytest.raises(TypeError):
        Dataset().parse()
    with pytest.raises(UnicodeDecodeError):
        Dataset().parse(data=b"\xff")


def test_graph_conversion_and_unsupported_conversion_inputs():
    from rdflib import ConjunctiveGraph, Graph

    graph = Graph().parse(data='<urn:s> <urn:p> "hello" .', format="turtle")
    assert set(Dataset.from_rdflib(graph).to_rdflib().default_graph) == set(graph)
    with pytest.raises(TypeError):
        Dataset.from_rdflib(object())
    with pytest.raises(NotImplementedError):
        Dataset.from_rdflib(ConjunctiveGraph())


def test_native_term_decoder_rejects_invalid_term_contract():
    from rdflib_turbo import _term

    assert _term(None) is None
    with pytest.raises(ValueError, match="Unknown native RDF term"):
        _term({"type": "unsupported", "value": "bad"})
