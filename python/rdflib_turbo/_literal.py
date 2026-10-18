"""Restore the state of native literals without repeating plain-string conversion."""

from rdflib import Literal


def restore(lexical, datatype, language):
    if datatype is not None:
        return Literal(lexical, lang=language, datatype=datatype, normalize=False)
    literal = str.__new__(Literal, lexical)
    literal._language = language
    literal._datatype = None
    literal._value = lexical
    literal._ill_typed = None
    return literal
