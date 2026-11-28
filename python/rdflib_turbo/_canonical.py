"""RDFC-1.0 canonical N-Quads and exact blank-node isomorphism over native storage.

Canonical output is rendered natively from the stored terms: literal lexical
forms are never normalized (ADRs 0006 and 0014). Blank-node labelling is
bounded by a caller-set work budget; exhausting it raises
``CanonicalizationBudgetExceeded`` instead of running for factorial time on
RDFC-1.0 "poison" inputs.
"""

from __future__ import annotations

from enum import StrEnum
from typing import TYPE_CHECKING, Final, Literal

from rdflib import Dataset as RDFLibDataset
from rdflib import Graph as RDFLibGraph

from ._native import CanonicalizationBudgetExceeded
from ._reads import encode_graph

if TYPE_CHECKING:
    from ._native import NativeStore

__all__ = [
    "DEFAULT_WORK_BUDGET",
    "CanonicalizationBudgetExceeded",
    "GraphCanonicalization",
    "HashAlgorithm",
]

DEFAULT_WORK_BUDGET: Final[int] = 1_000_000
"""Default RDFC-1.0 work units: Hash N-Degree Quads calls plus permutations."""


class HashAlgorithm(StrEnum):
    """The RDFC-1.0 hash functions."""

    SHA256 = "sha256"
    SHA384 = "sha384"


class _Identity(StrEnum):
    """RDF 1.1 identity for RDFC-1.0 output; RDFLib identity for comparison."""

    RDF11 = "rdf11"
    EXACT = "exact"


_Output = Literal["nquads", "store"]


def _work_budget(work_budget: int) -> int:
    if type(work_budget) is not int:
        raise TypeError("work_budget must be an int")
    if work_budget <= 0:
        raise ValueError("work_budget must be positive")
    return work_budget


def _native_call(
    graph: RDFLibGraph,
    *,
    hash_algorithm: HashAlgorithm,
    work_budget: int,
    identity: _Identity,
    output: _Output,
) -> object:
    """Run one native canonicalization over a native Graph or Dataset scope."""
    store = graph.store
    native: NativeStore = store.backend._native  # type: ignore[attr-defined]
    dataset = isinstance(graph, RDFLibDataset)
    scope = (dataset, None if dataset else encode_graph(store, graph))
    request = (HashAlgorithm(hash_algorithm).value, _work_budget(work_budget), identity, output)
    return native._canonical(scope, request)


def native_owner(graph: RDFLibGraph) -> RDFLibGraph:
    """The graph itself when native; an explicit native copy of an RDFLib graph."""
    if getattr(graph, "_turbo_native_graph", False):
        return graph
    from .graph import StoreDataset

    copy = StoreDataset.from_rdflib(graph)
    return copy if isinstance(graph, RDFLibDataset) else copy.default_graph


def exact_nquads(graph: RDFLibGraph, work_budget: int) -> str:
    """Canonical N-Quads under RDFLib term identity, for isomorphism tests."""
    text = _native_call(
        native_owner(graph),
        hash_algorithm=HashAlgorithm.SHA256,
        work_budget=work_budget,
        identity=_Identity.EXACT,
        output="nquads",
    )
    assert isinstance(text, str)
    return text


def canonical_store(graph: RDFLibGraph, work_budget: int) -> object:
    """A native store holding the graph's triples with canonical blank nodes."""
    return _native_call(
        native_owner(graph),
        hash_algorithm=HashAlgorithm.SHA256,
        work_budget=work_budget,
        identity=_Identity.EXACT,
        output="store",
    )


class GraphCanonicalization:
    """Canonical forms for native Graph and Dataset scopes."""

    def canonicalize(
        self,
        *,
        hash_algorithm: HashAlgorithm | str = HashAlgorithm.SHA256,
        work_budget: int = DEFAULT_WORK_BUDGET,
    ) -> str:
        """Return the RDFC-1.0 canonical N-Quads document of this scope.

        A Graph is canonicalized as a default-graph dataset of its triples; a
        Dataset keeps its graph names. ``hash_algorithm`` selects SHA-256 (the
        RDFC-1.0 default) or SHA-384. Lexical forms are the stored ones.
        """
        assert isinstance(self, RDFLibGraph)
        text = _native_call(
            self,
            hash_algorithm=HashAlgorithm(hash_algorithm),
            work_budget=work_budget,
            identity=_Identity.RDF11,
            output="nquads",
        )
        assert isinstance(text, str)
        return text

    def isomorphic(self, other: RDFLibGraph, *, work_budget: int = DEFAULT_WORK_BUDGET) -> bool:
        """Exact blank-node isomorphism, as ``rdflib.compare.isomorphic`` decides it.

        RDFLib's own ``Graph.isomorphic`` is an approximation that can report
        false positives; this answer is exact.
        """
        assert isinstance(self, RDFLibGraph)
        return exact_nquads(self, work_budget) == exact_nquads(other, work_budget)
