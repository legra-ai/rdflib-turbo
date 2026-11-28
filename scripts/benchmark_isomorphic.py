"""Compare ``rdflib.compare.isomorphic`` with rdflib-turbo's native RDFC-1.0 isomorphism.

Each fixture is one N-Triples document parsed twice per library: once as written
and once with every blank-node label renamed, so every comparison is a true
isomorphism that both libraries must confirm. Graph construction is outside the
timed region; each timing is one ``isomorphic(left, right)`` call. Medians of
``--repeats`` runs per arm are printed as a Markdown table; ``--output`` also
writes every sample as JSON.
"""

from __future__ import annotations

import argparse
import json
import platform
import statistics
import sys
import time
from collections.abc import Callable, Iterator
from dataclasses import asdict, dataclass
from importlib.metadata import version
from pathlib import Path

import rdflib
import rdflib.compare
import rdflib_turbo
import rdflib_turbo.compare

P = "<urn:bench:p>"
NEXT = "<urn:bench:next>"
VALUE = "<urn:bench:value>"


def records(count: int) -> Iterator[str]:
    """Ground subjects, each pointing at a blank-node record with literal fields."""
    for index in range(count):
        yield f"<urn:bench:s{index}> {P} _:r{index} ."
        yield f'_:r{index} {VALUE} "{index:05d}"^^<http://www.w3.org/2001/XMLSchema#integer> .'
        yield f'_:r{index} <urn:bench:label> "record {index}"@en .'


def lists(count: int, length: int) -> Iterator[str]:
    """``count`` RDF lists of ``length`` blank-node cells."""
    first = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#first>"
    rest = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#rest>"
    nil = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#nil>"
    for item in range(count):
        yield f"<urn:bench:list{item}> {P} _:l{item}c0 ."
        for cell in range(length):
            tail = nil if cell == length - 1 else f"_:l{item}c{cell + 1}"
            yield f'_:l{item}c{cell} {first} "{item}-{cell}" .'
            yield f"_:l{item}c{cell} {rest} {tail} ."


def rings(count: int, size: int) -> Iterator[str]:
    """Unanchored blank-node cycles: the hard case for colour refinement."""
    for ring in range(count):
        for node in range(size):
            yield f"_:g{ring}n{node} {NEXT} _:g{ring}n{(node + 1) % size} ."


def ground(count: int) -> Iterator[str]:
    for index in range(count):
        yield f'<urn:bench:s{index}> {P} "value {index}" .'


FIXTURES: dict[str, Callable[[], Iterator[str]]] = {
    "records-3000": lambda: records(1_000),
    "lists-100x25": lambda: lists(100, 25),
    "rings-3x4": lambda: rings(3, 4),
    "ground-20000": lambda: ground(20_000),
}


@dataclass(frozen=True)
class Row:
    fixture: str
    triples: int
    rdflib_ms: list[float]
    turbo_ms: list[float]

    @property
    def speedup(self) -> float:
        return statistics.median(self.rdflib_ms) / statistics.median(self.turbo_ms)


def renamed(document: str) -> str:
    return document.replace("_:", "_:x")


def timed(call: Callable[[], bool], repeats: int) -> list[float]:
    samples = []
    for _ in range(repeats):
        started = time.perf_counter()
        if not call():
            raise AssertionError("an isomorphic pair was reported as different")
        samples.append((time.perf_counter() - started) * 1_000)
    return samples


def measure(name: str, repeats: int) -> Row:
    document = "\n".join(FIXTURES[name]()) + "\n"

    def pair(factory: type[rdflib.Graph]) -> tuple[rdflib.Graph, rdflib.Graph]:
        return (
            factory().parse(data=document, format="nt"),
            factory().parse(data=renamed(document), format="nt"),
        )

    reference = pair(rdflib.Graph)
    turbo = pair(rdflib_turbo.Graph)
    return Row(
        fixture=name,
        triples=len(reference[0]),
        rdflib_ms=timed(lambda: rdflib.compare.isomorphic(*reference), repeats),
        turbo_ms=timed(lambda: rdflib_turbo.compare.isomorphic(*turbo), repeats),
    )


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repeats", type=int, default=5)
    parser.add_argument("--output", type=Path)
    arguments = parser.parse_args()
    rows = [measure(name, arguments.repeats) for name in FIXTURES]
    print(f"rdflib {version('rdflib')}, rdflib-turbo {version('rdflib-turbo')}, ", end="")
    print(f"CPython {platform.python_version()}, {platform.platform()}")
    print("| Fixture | Triples | RDFLib (ms) | rdflib-turbo (ms) | Speedup |")
    print("| --- | ---: | ---: | ---: | ---: |")
    for row in rows:
        print(
            f"| {row.fixture} | {row.triples} | {statistics.median(row.rdflib_ms):.2f} "
            f"| {statistics.median(row.turbo_ms):.2f} | {row.speedup:.1f}x |"
        )
    if arguments.output is not None:
        environment = {
            "rdflib": version("rdflib"),
            "rdflib_turbo": version("rdflib-turbo"),
            "python": sys.version,
            "platform": platform.platform(),
        }
        payload = {"environment": environment, "rows": [asdict(row) for row in rows]}
        arguments.output.write_text(json.dumps(payload, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
