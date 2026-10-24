"""Observe complete RDF state without imposing a document syntax boundary."""

from rdflib import Graph, Literal
from rdflib_turbo import StoreDataset


def term_key(value):
    return (
        type(value).__name__,
        str(value),
        str(value.datatype) if isinstance(value, Literal) and value.datatype is not None else None,
        value.language if isinstance(value, Literal) else None,
    )


def reference_dataset(owner):
    if hasattr(owner, "to_rdflib"):
        return owner.to_rdflib()
    return StoreDataset(store=owner).to_rdflib()


def physical_keys(owner):
    dataset = reference_dataset(owner)
    default = dataset.default_graph.identifier
    return {
        (*map(term_key, (s, p, o)), None if g == default else term_key(g))
        for s, p, o, g in dataset.quads((None,) * 4)
    }


def union_graph(owner):
    dataset = reference_dataset(owner)
    dataset.default_union = True
    result = Graph()
    for triple in dataset.triples((None, None, None)):
        result.add(triple)
    return result


def inference_outcome(owner):
    return owner.last_inference if isinstance(owner, StoreDataset) else owner._last_inference
