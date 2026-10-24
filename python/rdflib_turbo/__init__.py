"""RDFLib graphs and datasets over native exact-term storage."""

from importlib.metadata import version
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    from .graph import Graph, StoreDataset
    from .graph import StoreDataset as Dataset
    from .store import Store, Transaction, ValidationError

__version__ = version("rdflib-turbo")
__all__ = ["Dataset", "Graph", "Store", "StoreDataset", "Transaction", "ValidationError"]


def __getattr__(name: str) -> object:
    if name in {"Dataset", "Graph", "StoreDataset"}:
        from . import graph

        return graph.StoreDataset if name == "Dataset" else getattr(graph, name)
    if name in {"Store", "Transaction", "ValidationError"}:
        from . import store

        return getattr(store, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__() -> list[str]:
    return sorted(set(globals()) | set(__all__))
