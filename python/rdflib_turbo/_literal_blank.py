"""Render a reported blank-node closure through its actual source graph."""

from rdflib import BNode, Dataset, Graph, Literal, URIRef
from rdflib.namespace import OWL, RDF

_SAME_AS = OWL.sameAs


def _context(graph, node):
    for pattern in ((node, None, None, None), (None, None, node, None)):
        for _, _, _, context in graph.quads(pattern):
            if context is not None:
                return context if isinstance(context, Graph) else graph.get_context(context)
    raise LookupError(f"Cannot find node {node} in any named graph.")


def describe(graph, node, manager, recursion, cache, formatter):
    if isinstance(graph, Dataset):
        graph = _context(graph, node)
    assert isinstance(graph, Graph)
    assert isinstance(node, BNode)
    if recursion >= 12:
        return "<http://recursion.too.deep>"
    key = id(graph), str(node)
    if key in cache:
        return cache[key]
    predicates = list(graph.predicates(node))
    if not predicates:
        return "[ ]"

    def child(value):
        return formatter(graph, value, manager, _recursion=recursion + 1, _cache=cache)

    if RDF.first in predicates:
        return "( {} )".format(" ".join(child(item) for item in graph.items(node)))
    descriptions = {}
    for predicate in predicates:
        name = (
            predicate.n3(namespace_manager=manager)
            if isinstance(predicate, (Literal, BNode, URIRef))
            else str(predicate)
        )
        objects = list(graph.objects(node, predicate))
        if not objects:
            continue
        values = [
            "<self>" if predicate is _SAME_AS and value is node else child(value)
            for value in objects
        ]
        descriptions[name] = ", ".join(sorted(values)) if len(values) > 1 else values[0]
    if len(descriptions) > 1:
        body = " ; ".join(f"{name} {value}" for name, value in sorted(descriptions.items()))
    else:
        name, value = next(iter(descriptions.items()))
        body = f"{name} {value}"
    result = f"[ {body} ]"
    cache[key] = result
    return result
