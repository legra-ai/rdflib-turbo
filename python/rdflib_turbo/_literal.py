"""Restore the state of native literals without repeating plain-string conversion."""

from rdflib import Graph, Literal, URIRef
from rdflib.namespace import SH


def restore(lexical, datatype, language):
    if datatype is not None:
        return Literal(lexical, lang=language, datatype=datatype, normalize=False)
    literal = str.__new__(Literal, lexical)
    literal._language = language
    literal._datatype = None
    literal._value = lexical
    literal._ill_typed = None
    return literal


def restore_boolean(lexical, cached_value, assessment):
    """Deserialize an attested builtin Boolean; never repeat Literal conversion."""
    if type(lexical) is not str:
        raise TypeError("Boolean descriptor lexical value must be a string")
    if lexical not in ("false", "true"):
        raise ValueError("Expected a canonical Boolean descriptor word")
    if type(cached_value) is not bool or (assessment is not None and type(assessment) is not bool):
        raise TypeError("Boolean descriptor requires builtin bool and tri-state assessment")
    literal = str.__new__(Literal, lexical)
    literal._language = None
    literal._datatype = str.__new__(URIRef, "http://www.w3.org/2001/XMLSchema#boolean")
    literal._value = cached_value
    literal._ill_typed = assessment
    return literal


def _selected_stringify(graph, node, namespace_manager=None):
    """The selected Literal/IRI report consumer, independent of restoration."""
    manager = graph.namespace_manager if namespace_manager is None else namespace_manager
    if isinstance(manager, Graph):
        manager = manager.namespace_manager
    if manager is None or isinstance(manager, Graph):
        raise RuntimeError("Cannot stringify node, no namespaces known.")
    manager.bind("sh", SH, override=False, replace=False)
    if isinstance(node, URIRef):
        try:
            return node.n3(namespace_manager=manager)
        except Exception:
            return str(node)
    if not isinstance(node, Literal):
        # This branch is only a dynamically returned datatype, not a new
        # admission decision about the acquired selected Literal's class.
        return str(node)
    cached = None if node.value is None else str(node.value)
    lexical = str(node)
    if cached is not None:
        position = cached.find(" object at 0x")
        if position > 0:
            cached = cached[:position]
    value = (
        '"{}" = {}'.format(lexical, cached)
        if cached is not None and cached != lexical
        else '"{}"'.format(lexical)
    )
    language = ", lang={}".format(str(node.language)) if node.language else ""
    if node.datatype:
        if isinstance(node.datatype, (URIRef, Literal)):
            datatype = _selected_stringify(graph, node.datatype, manager)
        else:
            datatype = str(node.datatype)
        datatype = ", datatype={}".format(datatype)
    else:
        datatype = ""
    return "Literal({}{}{})".format(value, language, datatype)


def _selected_presentation(graph, node):
    try:
        text = _selected_stringify(graph, node)
    except (LookupError, ValueError):
        text = str(node)
    # This private prototype has a scalar String carrier. Do not dispatch a
    # codec/error handler or silently repair non-scalar Python code points.
    if not str.isascii(text) and any(
        0xD800 <= ord(point) <= 0xDFFF for point in str.__iter__(text)
    ):
        raise UnicodeError(
            "Selected report presentation requires lossless non-scalar text transport"
        )
    return text
