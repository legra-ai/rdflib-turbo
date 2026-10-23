"""Cold-process N-Triples serialization with exact-term checks after measurement."""

import argparse
import gc
import hashlib
import io
import json
import resource
import sys
import time
from collections import Counter
from importlib.metadata import version
from importlib.util import find_spec

MAX_ROWS = 100_000 - 3  # Three fixed controls keep the total at most100,000 triples.
COMPLETE_LIMIT = 1_000
XSD = "http://www.w3.org/2001/XMLSchema#"
BNODES = ("subject-id", "object-id")


def canonical(row):
    return json.dumps(row, ensure_ascii=False, separators=(",", ":"))


def fixture_rows(count):
    for index in range(count):
        subject = ("uri", f"urn:nt-benchmark:s:{index}")
        predicate = ("uri", f"urn:nt-benchmark:p:{index % 5}")
        obj = (
            ("literal", f"value-{index}", None, None),
            ("literal", f"value-{index}", XSD + "string", None),
            ("literal", f"漢字🌍-{index}", None, "fr"),
            ("literal", f"0{index}", XSD + "integer", None),
            ("uri", f"urn:nt-benchmark:o:{index % 17}"),
        )[index % 5]
        yield subject, predicate, obj
    yield (
        ("blank", "subject-id"),
        ("uri", "urn:nt-benchmark:control"),
        ("literal", 'tab\tline\n"\\', None, None),
    )
    yield (
        ("uri", "urn:nt-benchmark:control"),
        ("uri", "urn:nt-benchmark:blank"),
        ("blank", "object-id"),
    )
    yield (
        ("uri", "urn:nt-benchmark:control"),
        ("uri", "urn:nt-benchmark:typed"),
        ("literal", "same", XSD + "string", None),
    )


class Witness:
    """Order-independent cryptographic witness; not a complete large multiset proof."""

    def __init__(self, complete):
        self.counter = Counter() if complete else None
        self.count = self.total = self.xor = self.square = 0

    def add(self, row):
        key = canonical(row)
        value = int.from_bytes(hashlib.sha256(key.encode()).digest(), "big")
        self.count += 1
        self.total = (self.total + value) % (1 << 256)
        self.xor ^= value
        self.square = (self.square + value * value) % (1 << 256)
        if self.counter is not None:
            self.counter[key] += 1

    def record(self):
        result = {
            "count": self.count,
            "sha256_sum_mod_2_256": f"{self.total:064x}",
            "sha256_xor": f"{self.xor:064x}",
            "sha256_square_sum_mod_2_256": f"{self.square:064x}",
        }
        if self.counter is not None:
            result["complete_exact_multiset"] = sorted(self.counter.items())
        return result


def parse_witness(document, complete):
    from rdflib import BNode, Literal
    from rdflib.plugins.parsers.ntriples import W3CNTriplesParser

    def term(value):
        if isinstance(value, Literal):
            return (
                "literal",
                str(value),
                str(value.datatype) if value.datatype is not None else None,
                value.language,
            )
        return ("blank" if isinstance(value, BNode) else "uri", str(value))

    witness = Witness(complete)

    class Sink:
        def triple(self, subject, predicate, obj):
            witness.add(tuple(map(term, (subject, predicate, obj))))

    parser = W3CNTriplesParser(Sink(), bnode_context={name: BNode(name) for name in BNODES})
    parser.parse(io.StringIO(document.decode("utf-8") if isinstance(document, bytes) else document))
    return witness.record()


def fixture_document(count):
    # Base rows have only IRI subjects/objects; add blank controls programmatically
    # so ordinary parser document scoping cannot generate different control IDs.
    from rdflib import Literal, URIRef

    def render(term):
        if term[0] == "uri":
            return URIRef(term[1]).n3()
        return Literal(term[1], datatype=term[2], lang=term[3], normalize=False).n3()

    return "".join(
        " ".join(map(render, row)) + " .\n"
        for row in fixture_rows(count)
        if row[0][1].startswith("urn:nt-benchmark:s:")
    )


def make_graph(arm, scope, count):
    import rdflib
    from rdflib import BNode, Literal, URIRef

    # Both fixture parsing and later correctness parsing retain numeric lexical forms.
    rdflib.NORMALIZE_LITERALS = False
    if arm == "python":
        Dataset = rdflib.Dataset
    else:
        from rdflib_turbo import StoreDataset as Dataset
    owner = Dataset()
    graph = (
        owner.default_graph
        if scope == "default"
        else owner.graph(
            URIRef("urn:nt-benchmark:selected") if scope == "named" else BNode("scope-id")
        )
    )
    method_owner = type(graph).serialize.__module__
    expected_method = "rdflib_turbo._serialization" if arm == "candidate" else "rdflib.graph"
    assert method_owner == expected_method, (arm, method_owner, expected_method)
    text = fixture_document(count)
    input_hash = hashlib.sha256(text.encode()).hexdigest()
    graph.parse(data=text, format="nt")
    del text

    def term(value):
        if value[0] == "uri":
            return URIRef(value[1])
        if value[0] == "blank":
            return BNode(value[1])
        return Literal(value[1], datatype=value[2], lang=value[3], normalize=False)

    for row in fixture_rows(0):
        graph.add(tuple(map(term, row)))
    owner.graph(URIRef("urn:nt-benchmark:unrelated")).add(
        (
            URIRef("urn:nt-benchmark:excluded"),
            URIRef("urn:nt-benchmark:p"),
            Literal("must not leak"),
        )
    )
    assert len(graph) == count + 3
    return graph, owner, input_hash, method_owner


def peak_bytes():
    value = resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
    return value if sys.platform == "darwin" else value * 1024


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--arm", choices=("python", "prior", "candidate"), required=True)
    parser.add_argument("--scope", choices=("default", "named", "blank"), required=True)
    parser.add_argument("--encoding", choices=("str", "utf8"), required=True)
    parser.add_argument("--rows", type=int, required=True)
    args = parser.parse_args()
    assert 1 <= args.rows <= MAX_ROWS
    assert sys.version.split()[0] == "3.13.16", sys.version
    assert version("rdflib") == "7.6.0"
    assert find_spec("pyoxigraph") is None, "Benchmark runners must omit optional PyOxigraph"
    graph, _owner, input_hash, method_owner = make_graph(args.arm, args.scope, args.rows)
    gc.collect()
    before_peak = peak_bytes()
    encoding = None if args.encoding == "str" else "utf-8"
    start_wall, start_cpu = time.perf_counter_ns(), time.process_time_ns()
    output = graph.serialize(format="nt", encoding=encoding)
    cpu_ns, wall_ns = time.process_time_ns() - start_cpu, time.perf_counter_ns() - start_wall
    measured_peak = peak_bytes()
    assert type(output) is (str if encoding is None else bytes)
    # Every fact/term parse, hash and provenance operation below is outside timers
    # and the recorded peak; whole-child watchdog still covers the later audit.
    complete = args.rows <= COMPLETE_LIMIT
    actual = parse_witness(output, complete)
    expected = Witness(complete)
    for row in fixture_rows(args.rows):
        expected.add(row)
    expected = expected.record()
    assert actual == expected, (actual, expected)
    natives = sorted(
        str(getattr(module, "__file__", ""))
        for name, module in sys.modules.items()
        if name.startswith("rdflib_turbo")
        and str(getattr(module, "__file__", "")).endswith((".so", ".pyd"))
    )
    assert len(natives) == (0 if args.arm == "python" else 1), natives
    result = {
        "arm": args.arm,
        "scope": args.scope,
        "encoding": args.encoding,
        "base_rows": args.rows,
        "total_expected_rows": args.rows + 3,
        "serialize_wall_ns": wall_ns,
        "serialize_cpu_ns": cpu_ns,
        "process_peak_before_serialize_bytes": before_peak,
        "process_peak_after_serialize_before_audit_bytes": measured_peak,
        "peak_increment_bytes": max(0, measured_peak - before_peak),
        "rss_limitation": "process high-water includes imports/input setup; not isolated live serializer heap",
        "output_utf8_bytes": len(output.encode("utf-8"))
        if isinstance(output, str)
        else len(output),
        "raw_output_sha256": hashlib.sha256(
            output.encode("utf-8") if isinstance(output, str) else output
        ).hexdigest(),
        "fixture_document_sha256": input_hash,
        "serializer_method_module": method_owner,
        "correctness_kind": "complete exact lexical/ID multiset"
        if complete
        else "streaming count+SHA256 multiset witness (probabilistic, not complete proof)",
        "correctness": actual,
        "python": sys.version,
        "rdflib_version": version("rdflib"),
        "product_native_modules": natives,
        "pyoxigraph_loaded": False,
    }
    print(json.dumps(result, ensure_ascii=False))


if __name__ == "__main__":
    main()
