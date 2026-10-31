"""Two public flags use the original RDFLib module as their only authority."""

import subprocess
import sys
import warnings
from pathlib import Path

import pytest
import rdflib
import rdflib_turbo

FLAGS = ("NORMALIZE_LITERALS", "DAWG_LITERAL_COLLATION")


@pytest.fixture(autouse=True)
def restore_original_policy():
    original = {name: getattr(rdflib, name) for name in FLAGS}
    namespace = rdflib_turbo.__dict__
    missing = object()
    local = {name: namespace.get(name, missing) for name in FLAGS}
    yield
    for name, value in original.items():
        setattr(rdflib, name, value)
    # Restore only this test's preexisting module dictionary, including old shadow state.
    for name, value in local.items():
        if value is missing:
            namespace.pop(name, None)
        else:
            namespace[name] = value


@pytest.mark.parametrize("name", FLAGS)
@pytest.mark.parametrize("value", [False, True, None, 0, 1, "", "on", [], [1]])
def test_policy_assignment_shares_the_exact_original_value(name, value):
    setattr(rdflib_turbo, name, value)
    assert getattr(rdflib, name) is value
    assert getattr(rdflib_turbo, name) is value
    assert name not in vars(rdflib_turbo)
    assert name in dir(rdflib_turbo)


@pytest.mark.parametrize("name", FLAGS)
def test_policy_getter_reflects_later_original_writes(name):
    setattr(rdflib_turbo, name, False)
    marker = []
    setattr(rdflib, name, marker)
    assert getattr(rdflib_turbo, name) is marker
    marker.append(1)
    assert getattr(rdflib_turbo, name) == [1]
    setattr(rdflib, name, True)
    assert getattr(rdflib_turbo, name) is True


@pytest.mark.parametrize("name", FLAGS)
def test_custom_truth_is_not_evaluated_by_assignment_or_read(name):
    events = []
    error = ValueError("late policy truth")

    class Policy:
        def __bool__(self):
            events.append("truth")
            raise error

    policy = Policy()
    setattr(rdflib_turbo, name, policy)
    assert getattr(rdflib_turbo, name) is policy
    assert getattr(rdflib, name) is policy
    assert events == []
    with pytest.raises(ValueError) as failure:
        if name == "NORMALIZE_LITERALS":
            rdflib.Literal("01", datatype=rdflib.XSD.integer)
        else:
            rdflib.Literal(1).eq(rdflib.Literal("cake"))
    assert failure.value is error
    assert events == ["truth"]


@pytest.mark.parametrize("normalize, word", [(False, "01"), (True, "1"), (None, "01")])
def test_constructor_and_native_add_observe_the_same_normalization(normalize, word):
    rdflib_turbo.NORMALIZE_LITERALS = normalize
    value = rdflib_turbo.Literal("01", datatype=rdflib_turbo.XSD.integer)
    assert str(value) == word
    graph = rdflib_turbo.Graph()
    graph.add((rdflib_turbo.URIRef("urn:s"), rdflib_turbo.URIRef("urn:p"), value))
    restored = next(graph.objects())
    assert str(restored) == word
    assert restored.value == 1 and restored.ill_typed is False
    assert type(restored) is rdflib.Literal


def test_explicit_constructor_override_and_existing_word_are_preserved():
    rdflib_turbo.NORMALIZE_LITERALS = True
    value = rdflib_turbo.Literal("01", datatype=rdflib.XSD.integer, normalize=False)
    rdflib_turbo.NORMALIZE_LITERALS = False
    assert str(rdflib_turbo.Literal("01", datatype=rdflib.XSD.integer, normalize=True)) == "1"
    rdflib_turbo.NORMALIZE_LITERALS = True
    graph = rdflib_turbo.Graph()
    graph.add((rdflib.URIRef("urn:s"), rdflib.URIRef("urn:p"), value))
    assert str(next(graph.objects())) == "01"


def comparison(call):
    with warnings.catch_warnings(record=True) as captured:
        warnings.simplefilter("always")
        try:
            result = ("value", call())
        except Exception as error:
            result = ("error", type(error), error.args)
    return result, [(w.category, w.filename, w.lineno, str(w.message)) for w in captured]


@pytest.mark.parametrize("method", ["direct_gt", "python_gt", "value_eq", "rdf_eq"])
def test_dawg_comparison_matches_the_original_runtime(method):
    left = rdflib.Literal(1)
    right = rdflib.Literal("cake")
    calls = {
        "direct_gt": lambda: left.__gt__(right),
        "python_gt": lambda: left > right,
        "value_eq": lambda: left.eq(right),
        "rdf_eq": lambda: left == right,
    }
    rdflib.DAWG_LITERAL_COLLATION = True
    expected = comparison(calls[method])
    rdflib.DAWG_LITERAL_COLLATION = False
    rdflib_turbo.DAWG_LITERAL_COLLATION = True
    assert comparison(calls[method]) == expected


@pytest.mark.parametrize("name", FLAGS)
def test_delete_removes_the_original_key_without_a_default(name):
    delattr(rdflib_turbo, name)
    with pytest.raises(AttributeError) as original:
        getattr(rdflib, name)
    with pytest.raises(AttributeError) as forwarded:
        getattr(rdflib_turbo, name)
    assert forwarded.value.args == original.value.args
    if name == "NORMALIZE_LITERALS":
        with pytest.raises(AttributeError):
            rdflib.Literal("01", datatype=rdflib.XSD.integer)
        assert str(rdflib.Literal("01", datatype=rdflib.XSD.integer, normalize=False)) == "01"
    else:
        with pytest.raises(AttributeError):
            rdflib.Literal(1).eq(rdflib.Literal("cake"))


def test_policy_access_is_lazy_and_does_not_forward_unrelated_names():
    code = """
import sys
from pathlib import Path
sys.path.insert(0, str(Path(sys.argv[1]).resolve().parent.parent))
import rdflib_turbo
assert 'rdflib' not in sys.modules
assert 'rdflib_turbo._native' not in sys.modules
assert 'DAWG_LITERAL_COLLATION' in dir(rdflib_turbo)
rdflib_turbo.NORMALIZE_LITERALS = False
import rdflib
assert rdflib.NORMALIZE_LITERALS is False
assert rdflib_turbo.NORMALIZE_LITERALS is False
rdflib.DAWG_LITERAL_COLLATION = True
assert rdflib_turbo.DAWG_LITERAL_COLLATION is True
assert 'NORMALIZE_LITERALS' not in vars(rdflib_turbo)
assert 'DAWG_LITERAL_COLLATION' not in vars(rdflib_turbo)
marker = object()
rdflib_turbo._unrelated_policy_test = marker
assert rdflib_turbo._unrelated_policy_test is marker
assert '_unrelated_policy_test' not in vars(rdflib)
assert 'rdflib_turbo._native' not in sys.modules
assert 'rdflib_turbo.graph' not in sys.modules
"""
    result = subprocess.run(
        [sys.executable, "-I", "-B", "-c", code, str(Path(rdflib_turbo.__file__))],
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 0, result.stderr
