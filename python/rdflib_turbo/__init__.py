"""RDFLib graphs and datasets over native exact-term storage."""

import sys
from importlib.metadata import version
from types import ModuleType
from typing import TYPE_CHECKING

if TYPE_CHECKING:
    NORMALIZE_LITERALS: bool
    DAWG_LITERAL_COLLATION: bool

    from rdflib import (
        BRICK as BRICK,
    )
    from rdflib import (
        CSVW as CSVW,
    )
    from rdflib import (
        DC as DC,
    )
    from rdflib import (
        DCAT as DCAT,
    )
    from rdflib import (
        DCMITYPE as DCMITYPE,
    )
    from rdflib import (
        DCTERMS as DCTERMS,
    )
    from rdflib import (
        DOAP as DOAP,
    )
    from rdflib import (
        FOAF as FOAF,
    )
    from rdflib import (
        ODRL2 as ODRL2,
    )
    from rdflib import (
        ORG as ORG,
    )
    from rdflib import (
        OWL as OWL,
    )
    from rdflib import (
        PROF as PROF,
    )
    from rdflib import (
        PROV as PROV,
    )
    from rdflib import (
        QB as QB,
    )
    from rdflib import (
        RDF as RDF,
    )
    from rdflib import (
        RDFS as RDFS,
    )
    from rdflib import (
        SDO as SDO,
    )
    from rdflib import (
        SH as SH,
    )
    from rdflib import (
        SKOS as SKOS,
    )
    from rdflib import (
        SOSA as SOSA,
    )
    from rdflib import (
        SSN as SSN,
    )
    from rdflib import (
        TIME as TIME,
    )
    from rdflib import (
        VANN as VANN,
    )
    from rdflib import (
        VOID as VOID,
    )
    from rdflib import (
        XMLNS as XMLNS,
    )
    from rdflib import (
        XSD as XSD,
    )
    from rdflib import (
        BNode as BNode,
    )
    from rdflib import (
        IdentifiedNode as IdentifiedNode,
    )
    from rdflib import (
        Literal as Literal,
    )
    from rdflib import (
        Namespace as Namespace,
    )
    from rdflib import (
        Node as Node,
    )
    from rdflib import (
        URIRef as URIRef,
    )
    from rdflib import (
        Variable as Variable,
    )

    from .graph import Graph, StoreDataset
    from .graph import StoreDataset as Dataset
    from .store import Store, Transaction, ValidationError

__version__ = version("rdflib-turbo")
# bounded: the closed static term/namespace export contract, not module forwarding.
_RDFLIB_EXPORTS = (
    "URIRef",
    "BNode",
    "IdentifiedNode",
    "Literal",
    "Node",
    "Variable",
    "Namespace",
    "BRICK",
    "CSVW",
    "DC",
    "DCAT",
    "DCMITYPE",
    "DCTERMS",
    "DOAP",
    "FOAF",
    "ODRL2",
    "ORG",
    "OWL",
    "PROF",
    "PROV",
    "QB",
    "RDF",
    "RDFS",
    "SDO",
    "SH",
    "SKOS",
    "SOSA",
    "SSN",
    "TIME",
    "VANN",
    "VOID",
    "XMLNS",
    "XSD",
)
_RDFLIB_FLAGS = ("NORMALIZE_LITERALS", "DAWG_LITERAL_COLLATION")
__all__ = [
    "Dataset",
    "Graph",
    "Store",
    "StoreDataset",
    "Transaction",
    "ValidationError",
    "NORMALIZE_LITERALS",
    *_RDFLIB_EXPORTS,
]


def __getattr__(name: str) -> object:
    if name in _RDFLIB_EXPORTS or name in _RDFLIB_FLAGS:
        import rdflib

        return getattr(rdflib, name)
    if name in {"Dataset", "Graph", "StoreDataset"}:
        from . import graph

        return graph.StoreDataset if name == "Dataset" else getattr(graph, name)
    if name in {"Store", "Transaction", "ValidationError"}:
        from . import store

        return getattr(store, name)
    raise AttributeError(f"module {__name__!r} has no attribute {name!r}")


def __dir__() -> list[str]:
    return sorted(set(globals()) | set(__all__) | set(_RDFLIB_FLAGS))


class _RDFLibPolicyModule(ModuleType):
    """Keep the two original Literal policies on their original module."""

    def __setattr__(self, name: str, value: object) -> None:
        if name in _RDFLIB_FLAGS:
            import rdflib

            setattr(rdflib, name, value)
            return
        super().__setattr__(name, value)

    def __delattr__(self, name: str) -> None:
        if name in _RDFLIB_FLAGS:
            import rdflib

            delattr(rdflib, name)
            return
        super().__delattr__(name)


sys.modules[__name__].__class__ = _RDFLibPolicyModule
