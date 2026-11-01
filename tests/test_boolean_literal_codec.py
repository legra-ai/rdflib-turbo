"""Checked source state restores a literal without rerunning conversion."""

import warnings

import pytest
import rdflib
from rdflib import Literal, URIRef
from rdflib.namespace import XSD
from rdflib_turbo._literal import restore_boolean


@pytest.mark.parametrize("word", ["false", "true"])
@pytest.mark.parametrize("value", [False, True])
@pytest.mark.parametrize("assessment", [None, False, True])
def test_boolean_codec_preserves_exact_builtin_payload(word, value, assessment, monkeypatch):
    class UnreadablePolicy:
        def __bool__(self):
            raise AssertionError("restoration read global normalization")

    policy = UnreadablePolicy()
    monkeypatch.setattr(rdflib, "NORMALIZE_LITERALS", policy)
    with warnings.catch_warnings():
        warnings.simplefilter("error")
        result = restore_boolean(word, value, assessment)
    assert type(result) is Literal
    assert str(result) == word
    assert type(result.datatype) is URIRef and result.datatype == XSD.boolean
    assert result.language is None
    assert result.value is value and result.ill_typed is assessment
    assert rdflib.NORMALIZE_LITERALS is policy


@pytest.mark.parametrize("word", ["none", "1", "0", "TRUE", ""])
def test_boolean_codec_rejects_noncanonical_descriptor_word(word):
    with pytest.raises(ValueError, match="canonical Boolean"):
        restore_boolean(word, False, True)


@pytest.mark.parametrize("position", [0, 1, 2])
def test_boolean_codec_validates_payload_without_truth_coercion(position):
    class NotBoolean:
        def __bool__(self):
            raise AssertionError("descriptor coerced an arbitrary object")

    values = ["false", False, True]
    values[position] = NotBoolean()
    with pytest.raises(TypeError):
        restore_boolean(*values)
