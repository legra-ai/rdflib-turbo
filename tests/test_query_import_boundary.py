"""Text execution must not initialize RDFLib's unused Python SPARQL engine."""

import subprocess
import sys

import pytest
from rdflib import Graph as ReferenceGraph
from rdflib import URIRef
from rdflib.plugins.sparql import prepareQuery
from rdflib.plugins.sparql.sparql import Query
from rdflib_turbo import Graph


@pytest.mark.parametrize("execute", [False, True])
def test_native_import_and_text_query_do_not_import_python_sparql(execute):
    code = f"""
import sys
from rdflib_turbo import Graph
assert 'rdflib.plugins.sparql.operators' not in sys.modules
if {execute!r}:
    source = Graph().parse(data='<urn:s> <urn:p> <urn:o>.', format='nt')
    assert bool(source.query('ASK {{<urn:s> <urn:p> <urn:o>}}'))
    source.close()
    assert 'rdflib.plugins.sparql.operators' not in sys.modules
"""
    result = subprocess.run(
        [sys.executable, "-I", "-c", code], capture_output=True, text=True, check=False
    )
    assert result.returncode == 0, result.stderr


def test_prepared_query_with_a_string_base_keeps_its_actual_algebra():
    class StringQuery(str, Query):
        pass

    prepared = prepareQuery("SELECT ?s WHERE {?s <urn:p> <urn:o>}")
    query = str.__new__(StringQuery, "not valid SPARQL")
    Query.__init__(query, prepared.prologue, prepared.algebra)
    data = "<urn:s> <urn:p> <urn:o>."
    expected = list(ReferenceGraph().parse(data=data, format="nt").query(prepared))
    source = Graph().parse(data=data, format="nt")
    try:
        assert list(source.query(query)) == expected == [(URIRef("urn:s"),)]
    finally:
        source.close()
