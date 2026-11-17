"""Unknown sh: vocabulary is an annotation, exactly as in pySHACL."""

import pytest
from pyshacl import validate as original_validate
from pyshacl_turbo import validate
from rdflib import Graph
from rdflib.compare import isomorphic

PREFIX = "@prefix sh: <http://www.w3.org/ns/shacl#>.\n"
DATA = "<urn:a> <urn:p> 1 ."
UNKNOWN = """
<urn:S> a sh:NodeShape, sh:FutureShape; sh:targetNode <urn:a>, <urn:b>;
  sh:futureAnnotation "x";
  sh:property [ sh:path <urn:p>; sh:minCount 1; sh:futureConstraint 3;
                a sh:FuturePropertyThing ].
<urn:f> a sh:Function.
"""
JAVASCRIPT = [
    "<urn:S> sh:targetNode <urn:b>; sh:js [ sh:jsFunctionName 'f' ].",
    "<urn:S> sh:targetNode <urn:b>; sh:sparql [ a sh:JSConstraint ].",
    "<urn:f> a sh:JSFunction; sh:jsLibrary [ sh:jsLibraryURL 'x' ].",
    "<urn:S> sh:targetNode <urn:b>. <urn:T> a sh:JSTargetType.",
]


def graphs(shapes):
    return (
        Graph().parse(data=DATA, format="turtle"),
        Graph().parse(data=PREFIX + shapes, format="turtle"),
    )


@pytest.mark.parametrize("advanced", [False, True])
def test_unknown_terms_validate_exactly_like_the_reference(advanced):
    data, shapes = graphs(UNKNOWN)
    original = original_validate(data, shacl_graph=shapes, advanced=advanced)
    observed = validate(data, shacl_graph=shapes, advanced=advanced)
    assert original[0] is False
    assert observed[0] is original[0]
    assert isomorphic(original[1], observed[1])
    assert observed[2] == original[2]


@pytest.mark.parametrize("advanced", [False, True])
@pytest.mark.parametrize("shapes", JAVASCRIPT)
def test_shacl_javascript_fails_explicitly(shapes, advanced):
    data, shapes = graphs(shapes)
    with pytest.raises(NotImplementedError, match=r"sh:[jJ][sS]"):
        validate(data, shacl_graph=shapes, advanced=advanced)
