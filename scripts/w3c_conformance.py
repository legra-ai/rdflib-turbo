"""Grade rdflib-turbo and RDFLib against the vendored W3C RDF syntax test suites.

Every case runs through each library's public parsing API, as a user would call it.
Run ``python scripts/w3c_conformance.py --write`` to print both columns and rewrite
``tests/conformance-baseline.json`` after an intentional conformance change; the
known-failure ledger in that file is then checked by tests/test_w3c_conformance.py.
"""

from __future__ import annotations

import argparse
import hashlib
import json
from collections.abc import Callable, Iterator
from dataclasses import asdict, dataclass, field
from enum import Enum
from importlib.metadata import version
from pathlib import Path
from typing import Self

import rdflib
import rdflib_turbo
from rdflib import RDF, BNode, Graph, Namespace
from rdflib.collection import Collection
from rdflib.compare import isomorphic
from rdflib.graph import DATASET_DEFAULT_GRAPH_ID
from rdflib.term import Node

ROOT = Path(__file__).resolve().parents[1]
SUITE_ROOT = ROOT / "vendor" / "w3c-rdf-tests"
BASELINE = ROOT / "tests" / "conformance-baseline.json"
CHECKSUMS = SUITE_ROOT / "SHA256SUMS"
# The published location the manifests assume as their base IRI.
PUBLISHED_ROOT = "https://w3c.github.io/rdf-tests/"

MF = Namespace("http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#")
RDFT = Namespace("http://www.w3.org/ns/rdftest#")
# Reification vocabulary that turns a dataset into one graph for isomorphism checks.
QUAD = Namespace("urn:x-rdflib-turbo-conformance:")
DEFAULT_GRAPH = QUAD["default-graph"]


class Syntax(Enum):
    """An RDF concrete syntax and whether it carries named graphs."""

    TURTLE = ("turtle", False)
    TRIG = ("trig", True)
    NTRIPLES = ("nt", False)
    NQUADS = ("nquads", True)

    def __init__(self, rdflib_format: str, quads: bool) -> None:
        self.rdflib_format = rdflib_format
        self.quads = quads

    @property
    def result_format(self) -> str:
        """Syntax of the expected result of an evaluation test."""
        return "nquads" if self.quads else "nt"


class Suite(Enum):
    """One vendored manifest; its value is the manifest directory below SUITE_ROOT."""

    RDF11_TURTLE = "rdf/rdf11/rdf-turtle"
    RDF11_TRIG = "rdf/rdf11/rdf-trig"
    RDF11_NTRIPLES = "rdf/rdf11/rdf-n-triples"
    RDF11_NQUADS = "rdf/rdf11/rdf-n-quads"
    RDF12_TURTLE_SYNTAX = "rdf/rdf12/rdf-turtle/syntax"
    RDF12_TURTLE_EVAL = "rdf/rdf12/rdf-turtle/eval"
    RDF12_TRIG_SYNTAX = "rdf/rdf12/rdf-trig/syntax"
    RDF12_TRIG_EVAL = "rdf/rdf12/rdf-trig/eval"
    RDF12_NTRIPLES_SYNTAX = "rdf/rdf12/rdf-n-triples/syntax"
    RDF12_NQUADS_SYNTAX = "rdf/rdf12/rdf-n-quads/syntax"

    @property
    def syntax(self) -> Syntax:
        """The concrete syntax every case of this suite is written in."""
        family = self.value.split("/")[2]
        return {
            "rdf-turtle": Syntax.TURTLE,
            "rdf-trig": Syntax.TRIG,
            "rdf-n-triples": Syntax.NTRIPLES,
            "rdf-n-quads": Syntax.NQUADS,
        }[family]

    @property
    def directory(self) -> Path:
        return SUITE_ROOT / self.value

    @property
    def base(self) -> str:
        """The manifest's mf:assumedTestBase, which every action is parsed against."""
        return f"{PUBLISHED_ROOT}{self.value}/"

    def cases(self) -> Iterator[ConformanceCase]:
        """Read this manifest's entries in their published order."""
        # bounded: one manifest of at most a few hundred entries.
        manifest = Graph().parse(
            self.directory / "manifest.ttl", format="turtle", publicID=self.base + "manifest.ttl"
        )
        root = next(manifest.subjects(RDF.type, MF.Manifest))
        entries = manifest.value(root, MF.entries, any=False)
        if entries is None:
            raise ValueError(f"{self.name} manifest has no mf:entries")
        for entry in Collection(manifest, entries):
            yield ConformanceCase.from_manifest(self, manifest, entry)

    def local(self, iri: Node | None) -> Path:
        """Map a published action/result IRI back to its vendored file."""
        text = str(iri)
        if not text.startswith(self.base):
            raise ValueError(f"{text} is outside the {self.name} suite")
        return self.directory / text.removeprefix(self.base)


class CaseKind(Enum):
    """The rdft test types the harness grades; the value is the type's local name suffix."""

    POSITIVE_SYNTAX = "PositiveSyntax"
    NEGATIVE_SYNTAX = "NegativeSyntax"
    EVAL = "Eval"
    NEGATIVE_EVAL = "NegativeEval"

    @classmethod
    def from_type(cls, test_type: Node | None) -> Self:
        name = str(test_type).removeprefix(str(RDFT))
        for prefix in ("TestTurtle", "TestTrig", "TestNTriples", "TestNQuads"):
            if name.startswith(prefix):
                return cls(name.removeprefix(prefix))
        raise ValueError(f"Unsupported rdf-tests type {test_type}")

    @property
    def must_parse(self) -> bool:
        return self in (CaseKind.POSITIVE_SYNTAX, CaseKind.EVAL)


@dataclass(frozen=True)
class CaseId:
    """A case identity that stays stable across engines and baseline revisions."""

    suite: Suite
    name: str

    def __str__(self) -> str:
        return f"{self.suite.name}/{self.name}"


@dataclass(frozen=True)
class ConformanceCase:
    """One manifest entry: an input document, its expectation and its base IRI."""

    id: CaseId
    label: str
    kind: CaseKind
    action: Path
    result: Path | None
    base: str

    @classmethod
    def from_manifest(cls, suite: Suite, manifest: Graph, entry: Node) -> Self:
        action = manifest.value(entry, MF.action)
        result = manifest.value(entry, MF.result)
        return cls(
            # The entry IRI's fragment is unique per manifest; mf:name is not.
            id=CaseId(suite, str(entry).rpartition("#")[2]),
            label=str(manifest.value(entry, MF.name)),
            kind=CaseKind.from_type(manifest.value(entry, RDF.type, any=False)),
            action=suite.local(action),
            result=None if result is None else suite.local(result),
            base=str(action),
        )

    @property
    def syntax(self) -> Syntax:
        return self.id.suite.syntax


class Engine(Enum):
    """A library under test, reached only through its public Graph/Dataset classes."""

    TURBO = "turbo"
    RDFLIB = "rdflib"

    def parse(self, path: Path, syntax: Syntax, base: str) -> Graph:
        factory: Callable[[], Graph]
        if self is Engine.TURBO:
            factory = rdflib_turbo.Dataset if syntax.quads else rdflib_turbo.Graph
        else:
            factory = rdflib.Dataset if syntax.quads else rdflib.Graph
        return factory().parse(source=str(path), format=syntax.rdflib_format, publicID=base)

    def parse_text(self, nquads: str) -> Graph:
        """Parse an inline N-Quads document; used by the harness's own tests."""
        factory = rdflib_turbo.Dataset if self is Engine.TURBO else rdflib.Dataset
        return factory().parse(data=nquads, format="nquads")

    def grade(self, case: ConformanceCase) -> CaseOutcome:
        """Run one case; any exception from a parse counts as a rejection."""
        try:
            actual = self.parse(case.action, case.syntax, case.base)
        except Exception as error:  # every rejection is a negative-case pass
            if case.kind.must_parse:
                return CaseOutcome(case.id, self, False, f"rejected: {type(error).__name__}")
            return CaseOutcome(case.id, self, True, "rejected")
        if not case.kind.must_parse:
            return CaseOutcome(case.id, self, False, "accepted invalid input")
        if case.kind is not CaseKind.EVAL:
            return CaseOutcome(case.id, self, True, "parsed")
        return self.compare(case, actual)

    def compare(self, case: ConformanceCase, actual: Graph) -> CaseOutcome:
        """Each engine reads the expected N-Triples/N-Quads with its own simplest parser."""
        assert case.result is not None, f"{case.id} has no expected result"
        try:
            expected = self.parse(case.result, Syntax.NQUADS, case.base)
        except Exception as error:  # an unreadable expectation is a failure
            return CaseOutcome(case.id, self, False, f"expected unreadable: {error!r}"[:200])
        if QuadSet.of(actual).isomorphic(QuadSet.of(expected)):
            return CaseOutcome(case.id, self, True, "isomorphic")
        return CaseOutcome(case.id, self, False, "not isomorphic to the expected result")


Quad = tuple[Node, Node, Node, Node]


@dataclass(frozen=True)
class QuadSet:
    """The statements of a parsed document, with the default graph made explicit."""

    quads: frozenset[Quad]

    @classmethod
    def of(cls, graph: Graph) -> Self:
        # bounded: a single W3C test document of at most a few hundred statements.
        if not isinstance(graph, rdflib.Dataset):
            return cls(frozenset((s, p, o, DEFAULT_GRAPH) for s, p, o in graph))
        quads: set[Quad] = set()
        for subject, predicate, obj, context in graph.quads((None, None, None, None)):
            name = context.identifier if isinstance(context, Graph) else context
            if name is None or name == DATASET_DEFAULT_GRAPH_ID:
                name = DEFAULT_GRAPH
            quads.add((subject, predicate, obj, name))
        return cls(frozenset(quads))

    @property
    def has_blank_nodes(self) -> bool:
        return any(isinstance(term, BNode) for quad in self.quads for term in quad)

    def reified(self) -> Graph:
        """One graph whose isomorphism classes are exactly the dataset's."""
        graph = Graph()
        for subject, predicate, obj, name in self.quads:
            statement = BNode()
            graph.add((statement, QUAD.subject, subject))
            graph.add((statement, QUAD.predicate, predicate))
            graph.add((statement, QUAD.object, obj))
            graph.add((statement, QUAD.graph, name))
        return graph

    def isomorphic(self, other: QuadSet) -> bool:
        if len(self.quads) != len(other.quads):
            return False
        if not (self.has_blank_nodes or other.has_blank_nodes):
            return self.quads == other.quads
        return isomorphic(self.reified(), other.reified())


@dataclass(frozen=True)
class CaseOutcome:
    case: CaseId
    engine: Engine
    passed: bool
    detail: str


@dataclass(frozen=True)
class SuiteCounts:
    """Per-suite pass counts, the unit the ratchet compares."""

    total: int
    turbo: int
    rdflib: int


@dataclass(frozen=True)
class KnownFailure:
    """A ledgered turbo failure; rdflib_passes states whether turbo is worse here."""

    suite: Suite
    case: str
    reason: str
    rdflib_passes: bool

    @property
    def id(self) -> CaseId:
        return CaseId(self.suite, self.case)

    def to_json(self) -> dict[str, str | bool]:
        return {
            "suite": self.suite.name,
            "case": self.case,
            "rdflib_passes": self.rdflib_passes,
            "reason": self.reason,
        }

    @classmethod
    def from_json(cls, row: dict[str, str | bool]) -> Self:
        return cls(
            suite=Suite[str(row["suite"])],
            case=str(row["case"]),
            reason=str(row["reason"]),
            rdflib_passes=bool(row["rdflib_passes"]),
        )


@dataclass(frozen=True)
class Baseline:
    """The committed ratchet: exact per-suite counts and the turbo failure ledger."""

    rdflib_version: str
    suites: dict[Suite, SuiteCounts]
    known_failures: tuple[KnownFailure, ...]

    @classmethod
    def load(cls, path: Path = BASELINE) -> Self:
        data = json.loads(path.read_text(encoding="utf-8"))
        return cls(
            rdflib_version=data["rdflib_version"],
            suites={Suite[name]: SuiteCounts(**row) for name, row in data["suites"].items()},
            known_failures=tuple(KnownFailure.from_json(row) for row in data["known_failures"]),
        )

    def write(self, path: Path = BASELINE) -> None:
        data = {
            "rdflib_version": self.rdflib_version,
            "suites": {suite.name: asdict(counts) for suite, counts in self.suites.items()},
            "known_failures": [failure.to_json() for failure in self.known_failures],
        }
        path.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")

    def ledger(self) -> dict[CaseId, KnownFailure]:
        return {failure.id: failure for failure in self.known_failures}


@dataclass
class Report:
    """Both engines' outcomes for every vendored case."""

    outcomes: dict[CaseId, dict[Engine, CaseOutcome]] = field(default_factory=dict)

    @classmethod
    def run(cls) -> Self:
        # bounded: one small outcome record per vendored case (under two thousand).
        report = cls()
        for suite in Suite:
            for case in suite.cases():
                report.outcomes[case.id] = {engine: engine.grade(case) for engine in Engine}
        return report

    def counts(self) -> dict[Suite, SuiteCounts]:
        counts: dict[Suite, SuiteCounts] = {}
        for suite in Suite:
            rows = [row for case, row in self.outcomes.items() if case.suite is suite]
            counts[suite] = SuiteCounts(
                total=len(rows),
                turbo=sum(row[Engine.TURBO].passed for row in rows),
                rdflib=sum(row[Engine.RDFLIB].passed for row in rows),
            )
        return counts

    def failures(self, engine: Engine) -> dict[CaseId, CaseOutcome]:
        return {case: row[engine] for case, row in self.outcomes.items() if not row[engine].passed}

    def rdflib_passes(self, case: CaseId) -> bool:
        return self.outcomes[case][Engine.RDFLIB].passed

    def table(self) -> str:
        lines = ["| Suite | Cases | rdflib-turbo | RDFLib |", "| --- | ---: | ---: | ---: |"]
        for suite, counts in self.counts().items():
            lines.append(f"| {suite.name} | {counts.total} | {counts.turbo} | {counts.rdflib} |")
        return "\n".join(lines)

    def baseline(self, previous: Baseline | None) -> Baseline:
        """A new baseline that keeps the reasons already ledgered for unchanged failures."""
        reasons = {} if previous is None else previous.ledger()
        failures = tuple(
            KnownFailure(
                suite=case.suite,
                case=case.name,
                reason=reasons[case].reason if case in reasons else f"TODO: {outcome.detail}",
                rdflib_passes=self.rdflib_passes(case),
            )
            for case, outcome in self.failures(Engine.TURBO).items()
        )
        return Baseline(version("rdflib"), self.counts(), failures)


def checksum_lines() -> Iterator[str]:
    """SHA256SUMS lines for every vendored file, in byte-wise path order."""
    excluded = {CHECKSUMS, SUITE_ROOT / "README.md"}
    # bounded: the fixed vendored file list (under two thousand paths).
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
    print(report.table())
    if arguments.write:
        previous = Baseline.load() if BASELINE.exists() else None
        report.baseline(previous).write()


if __name__ == "__main__":
    main()
