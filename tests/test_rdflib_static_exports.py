"""Static RDFLib objects share identity without activating native graph code."""

import subprocess
import sys
from pathlib import Path

import pytest
import rdflib
import rdflib_turbo

STATIC_EXPORTS = (
    "URIRef",
    "BNode",
    "IdentifiedNode",
    "Literal",
    "Node",
    "Variable",
    "Namespace",
    "BRICK",
    "CSVW",
    "DC",
    "DCAT",
    "DCMITYPE",
    "DCTERMS",
    "DOAP",
    "FOAF",
    "ODRL2",
    "ORG",
    "OWL",
    "PROF",
    "PROV",
    "QB",
    "RDF",
    "RDFS",
    "SDO",
    "SH",
    "SKOS",
    "SOSA",
    "SSN",
    "TIME",
    "VANN",
    "VOID",
    "XMLNS",
    "XSD",
)


@pytest.mark.parametrize("name", STATIC_EXPORTS)
def test_static_export_is_the_original_object(name):
    scope = {}
    exec(f"from rdflib_turbo import {name}", scope)
    assert scope[name] is getattr(rdflib, name)
    assert name in rdflib_turbo.__all__
    assert name in dir(rdflib_turbo)


def test_common_imports_and_terms_work_with_native_graph():
    from rdflib_turbo import RDF, RDFS, XSD, BNode, Graph, Literal, Namespace, Variable

    namespace = Namespace("urn:static-exports:")
    graph = Graph(identifier=namespace.graph)
    value = Literal("01", datatype=XSD.integer, normalize=False)
    blank = BNode("fixed")
    graph.add((namespace.subject, namespace.predicate, value))
    assert graph.identifier == namespace.graph
    assert graph.store.backend._native is not None
    assert (namespace.subject, namespace.predicate, value) in graph
    assert str(value) == "01" and value.value == 1
    assert value.ill_typed is False
    assert isinstance(blank, rdflib.BNode)
    assert Variable("v").n3() == "?v"
    assert RDF is rdflib.RDF and RDF.type == rdflib.RDF.type
    assert RDFS is rdflib.RDFS and RDFS.Class == rdflib.RDFS.Class


def test_static_exports_do_not_activate_native_graph_modules():
    source = """
import sys
from pathlib import Path
sys.path.insert(0, str(Path(sys.argv[1]).resolve().parent.parent))
import rdflib_turbo
assert Path(rdflib_turbo.__file__).resolve() == Path(sys.argv[1]).resolve()
assert 'rdflib' not in sys.modules
assert 'rdflib_turbo._native' not in sys.modules
for name in sys.argv[2:]:
    scope = {}
    exec(f'from rdflib_turbo import {name}', scope)
    import rdflib
    assert scope[name] is getattr(rdflib, name)
assert 'rdflib_turbo._native' not in sys.modules
assert 'rdflib_turbo.graph' not in sys.modules
assert 'rdflib_turbo.store' not in sys.modules
"""
    result = subprocess.run(
        [
            sys.executable,
            "-I",
            "-B",
            "-c",
            source,
            str(Path(rdflib_turbo.__file__)),
            *STATIC_EXPORTS,
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr


def test_static_aliases_do_not_substitute_unsupported_graph_or_policy_contracts():
    assert "ConjunctiveGraph" not in rdflib_turbo.__all__
    for name in ("NORMALIZE_LITERALS", "DAWG_LITERAL_COLLATION"):
        assert (name in rdflib_turbo.__all__) == (name in rdflib.__all__)
    with pytest.raises(AttributeError, match=r"no attribute.*missing_static_export"):
        getattr(rdflib_turbo, "missing_static_export")
