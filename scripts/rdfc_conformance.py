"""Grade rdflib-turbo's RDFC-1.0 canonicalization against the vendored W3C rdf-canon suite.

Every case runs through the public ``Dataset.canonicalize`` API. RDFLib has no
RDFC-1.0 implementation; its column grades the closest equivalent it has, the
``rdflib.compare.isomorphic`` check that an evaluation case's input and
canonical result are the same dataset. Map and poison cases have no RDFLib
equivalent and are not graded for it. Run ``python scripts/rdfc_conformance.py
--write`` to rewrite the ``rdfc10`` section of ``tests/conformance-baseline.json``.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from collections.abc import Iterator
from dataclasses import asdict, dataclass
from enum import Enum
from pathlib import Path
from typing import Self

import rdflib
import rdflib_turbo
from rdflib import RDF, Graph, Namespace
from rdflib.collection import Collection
from rdflib.compare import isomorphic
from rdflib.graph import DATASET_DEFAULT_GRAPH_ID
from rdflib.term import Node
from rdflib_turbo import CanonicalizationBudgetExceeded, HashAlgorithm
from rdflib_turbo._native import NativeDataset

ROOT = Path(__file__).resolve().parents[1]
SUITE_ROOT = ROOT / "vendor" / "w3c-rdf-canon"
TESTS = SUITE_ROOT / "tests"
BASELINE = ROOT / "tests" / "conformance-baseline.json"
CHECKSUMS = SUITE_ROOT / "SHA256SUMS"
SECTION = "rdfc10"
# The published location the manifest's relative IRIs resolve against.
PUBLISHED_ROOT = "https://w3c.github.io/rdf-canon/tests/"

MF = Namespace("http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#")
RDFC = Namespace("https://w3c.github.io/rdf-canon/tests/vocab#")


class CaseKind(Enum):
    """The rdf-canon test types; the value is the type's local name."""

    EVAL = "RDFC10EvalTest"
    MAP = "RDFC10MapTest"
    NEGATIVE = "RDFC10NegativeEvalTest"


@dataclass(frozen=True)
class CanonCase:
    """One manifest entry: its input, expectation and hash algorithm."""

    name: str
    label: str
    kind: CaseKind
    action: Path
    result: Path | None
    hash_algorithm: HashAlgorithm

    @classmethod
    def from_manifest(cls, manifest: Graph, entry: Node) -> Self:
        kind = CaseKind(str(manifest.value(entry, RDF.type, any=False)).removeprefix(str(RDFC)))
        result = manifest.value(entry, MF.result)
        hash_name = manifest.value(entry, RDFC.hashAlgorithm)
        return cls(
            name=str(entry).rpartition("#")[2],
            label=str(manifest.value(entry, MF.name)),
            kind=kind,
            action=local(manifest.value(entry, MF.action)),
            result=None if result is None else local(result),
            hash_algorithm=HashAlgorithm.SHA256
            if hash_name is None
            else HashAlgorithm(str(hash_name).lower()),
        )


def local(iri: Node | None) -> Path:
    """Map a published action/result IRI back to its vendored file."""
    text = str(iri)
    if not text.startswith(PUBLISHED_ROOT):
        raise ValueError(f"{text} is outside the rdf-canon suite")
    return TESTS / text.removeprefix(PUBLISHED_ROOT)


def cases() -> Iterator[CanonCase]:
    """Read the manifest's entries in their published order."""
    # bounded: one manifest of under a hundred entries.
    manifest = Graph().parse(
        TESTS / "manifest.ttl", format="turtle", publicID=PUBLISHED_ROOT + "manifest"
    )
    root = next(manifest.subjects(RDF.type, MF.Manifest))
    entries = manifest.value(root, MF.entries, any=False)
    if entries is None:
        raise ValueError("rdf-canon manifest has no mf:entries")
    for entry in Collection(manifest, entries):
        yield CanonCase.from_manifest(manifest, entry)


def labelled_quads(text: str) -> frozenset[str]:
    """N-Quads statements with blank nodes kept as their document labels.

    The native loader reads every vector, including IRIs RDFLib's N-Quads
    parser rejects, and one serializer writes both sides of the comparison.
    """
    dataset = NativeDataset()
    dataset.load(text, "nquads", preserve_blank_nodes=True)
    # bounded: a single W3C test document of at most a few hundred statements.
    return frozenset(dataset.dump("nquads").splitlines())


@dataclass(frozen=True)
class CaseOutcome:
    case: str
    passed: bool
    detail: str


def canonical_text(case: CanonCase) -> str:
    dataset = rdflib_turbo.Dataset().parse(source=str(case.action), format="nquads")
    return dataset.canonicalize(hash_algorithm=case.hash_algorithm)


def grade_turbo(case: CanonCase) -> CaseOutcome:
    """Grade one case through the public canonicalization API."""
    try:
        text = canonical_text(case)
    except CanonicalizationBudgetExceeded:
        if case.kind is CaseKind.NEGATIVE:
            return CaseOutcome(case.name, True, "work budget exhausted")
        return CaseOutcome(case.name, False, "work budget exhausted")
    if case.kind is CaseKind.NEGATIVE:
        return CaseOutcome(case.name, False, "poison input canonicalized")
    assert case.result is not None, f"{case.name} has no expected result"
    if case.kind is CaseKind.EVAL:
        expected = case.result.read_text(encoding="utf-8")
        return CaseOutcome(case.name, text == expected, "canonical N-Quads compared")
    mapping = json.loads(case.result.read_text(encoding="utf-8"))
    source = case.action.read_text(encoding="utf-8")
    for original, issued in mapping.items():
        source = source.replace(f"_:{original} ", f"_:{issued} ")
    passed = labelled_quads(text) == labelled_quads(source)
    return CaseOutcome(case.name, passed, "issued identifiers compared")


def named_graphs(path: Path) -> bool:
    """Whether an N-Quads vector names a graph; RDFLib compares only graphs."""
    dataset = rdflib_turbo.Dataset().parse(source=str(path), format="nquads")
    return any(True for _ in dataset.graphs() if _.identifier != DATASET_DEFAULT_GRAPH_ID)


def grade_rdflib(case: CanonCase) -> CaseOutcome | None:
    """RDFLib's closest equivalent: input and canonical result are isomorphic graphs.

    Named-graph cases have no RDFLib equivalent (its comparison reads triples).
    A vector RDFLib cannot parse is a failure.
    """
    if case.kind is not CaseKind.EVAL or case.result is None or named_graphs(case.action):
        return None
    try:
        actual = rdflib.Graph().parse(source=str(case.action), format="nt")
        expected = rdflib.Graph().parse(source=str(case.result), format="nt")
    except Exception as error:  # an unreadable vector is a failure
        return CaseOutcome(case.name, False, f"unreadable: {type(error).__name__}")
    passed = isomorphic(actual, expected)
    return CaseOutcome(case.name, passed, "isomorphic" if passed else "not isomorphic")


@dataclass(frozen=True)
class KnownFailure:
    case: str
    reason: str


@dataclass(frozen=True)
class SuiteCounts:
    """Pinned counts: turbo over every case, RDFLib over the cases it can grade."""

    total: int
    turbo: int
    rdflib_graded: int
    rdflib: int


@dataclass(frozen=True)
class Baseline:
    counts: SuiteCounts
    known_failures: tuple[KnownFailure, ...]

    @classmethod
    def load(cls, path: Path = BASELINE) -> Self:
        section = json.loads(path.read_text(encoding="utf-8"))[SECTION]
        failures = tuple(KnownFailure(**row) for row in section.pop("known_failures"))
        return cls(SuiteCounts(**section), failures)

    def write(self, path: Path = BASELINE) -> None:
        """Rewrite only this suite's section, preserving the syntax-suite ratchet."""
        data = json.loads(path.read_text(encoding="utf-8"))
        data[SECTION] = {
            **asdict(self.counts),
            "known_failures": [asdict(failure) for failure in self.known_failures],
        }
        path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")


@dataclass(frozen=True)
class Report:
    turbo: dict[str, CaseOutcome]
    rdflib: dict[str, CaseOutcome]

    @classmethod
    def run(cls) -> Self:
        # bounded: one outcome record per vendored case (under a hundred).
        turbo: dict[str, CaseOutcome] = {}
        reference: dict[str, CaseOutcome] = {}
        for case in cases():
            turbo[case.name] = grade_turbo(case)
            outcome = grade_rdflib(case)
            if outcome is not None:
                reference[case.name] = outcome
        return cls(turbo, reference)

    def counts(self) -> SuiteCounts:
        return SuiteCounts(
            total=len(self.turbo),
            turbo=sum(outcome.passed for outcome in self.turbo.values()),
            rdflib_graded=len(self.rdflib),
            rdflib=sum(outcome.passed for outcome in self.rdflib.values()),
        )

    def failures(self) -> dict[str, CaseOutcome]:
        return {name: outcome for name, outcome in self.turbo.items() if not outcome.passed}

    def baseline(self, previous: Baseline | None) -> Baseline:
        reasons = {} if previous is None else {f.case: f.reason for f in previous.known_failures}
        failures = tuple(
            KnownFailure(name, reasons.get(name, f"TODO: {outcome.detail}"))
            for name, outcome in self.failures().items()
        )
        return Baseline(self.counts(), failures)


def checksum_lines() -> Iterator[str]:
    """SHA256SUMS lines for every vendored file, in byte-wise path order."""
    excluded = {CHECKSUMS, SUITE_ROOT / "README.md"}
    # bounded: the fixed vendored file list (under two hundred paths).
    files = sorted(
        (path.relative_to(SUITE_ROOT).as_posix(), path)
        for path in SUITE_ROOT.rglob("*")
        if path.is_file() and path not in excluded
    )
    for name, path in files:
        yield f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {name}"


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true", help="rewrite the committed baseline")
    arguments = parser.parse_args()
    report = Report.run()
    counts = report.counts()
    print(f"RDFC10: {counts.turbo}/{counts.total} rdflib-turbo; ", end="")
    print(f"RDFLib isomorphism {counts.rdflib}/{counts.rdflib_graded}")
    if arguments.write:
        previous = Baseline.load() if SECTION in json.loads(BASELINE.read_text()) else None
        report.baseline(previous).write()


if __name__ == "__main__":
    main()
