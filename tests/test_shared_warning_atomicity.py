"""Warning policy is evaluated before committing native rule mutations."""

import os
import subprocess
import sys
import warnings
from pathlib import Path

import pyshacl_turbo as turbo
import pytest
from rdflib import Graph
from rdflib_turbo import Dataset

SHAPES = """@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix ex: <urn:> .
ex:S sh:targetNode ex:a; sh:rule ex:R.
ex:R a sh:TripleRule; sh:condition ex:C;
  sh:subject sh:this; sh:predicate ex:derived; sh:object true.
ex:C sh:node ex:C.
"""
SOURCE = "@prefix ex: <urn:> . ex:a ex:p ex:b."


def program(source=SHAPES):
    return turbo.compile_shapes(Graph().parse(data=source, format="turtle"), advanced=True)


def snapshot(data):
    return data.serialize(), tuple(data.namespaces())


def test_warning_error_leaves_native_data_and_namespaces_unchanged():
    data = Dataset().parse(data=SOURCE)
    compiled = program()
    before = snapshot(data)
    with warnings.catch_warnings():
        warnings.simplefilter("error", turbo.ShapeRecursionWarning)
        with pytest.raises(turbo.ShapeRecursionWarning, match="Recursive Shape"):
            compiled.validate(data, inplace=True)
    assert snapshot(data) == before


def call_at_same_warning_location(compiled, data):
    return compiled.validate(data, inplace=True)


def test_native_warning_category_message_and_exact_location_match_graph():
    compiled = program()
    captures = []
    for data in (Graph().parse(data=SOURCE, format="turtle"), Dataset().parse(data=SOURCE)):
        with warnings.catch_warnings(record=True) as emitted:
            warnings.simplefilter("always", turbo.ShapeRecursionWarning)
            call_at_same_warning_location(compiled, data)
        captures.append(
            [
                (warning.category, str(warning.message), warning.filename, warning.lineno)
                for warning in emitted
            ]
        )
    assert len(captures[0]) == 1
    assert captures[0] == captures[1]
    assert captures[1][0][0] is turbo.ShapeRecursionWarning


def test_provider_emits_warnings_once_and_returns_no_late_messages():
    compiled = program()
    data = Dataset().parse(data=SOURCE)
    # Initialize the provider without mutating the receiver.
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", turbo.ShapeRecursionWarning)
        compiled.validate(data)
    with warnings.catch_warnings(record=True) as emitted:
        warnings.simplefilter("always", turbo.ShapeRecursionWarning)
        result = compiled._shared_program.validate(data._native, "{}", True)
    assert len(emitted) == 1
    assert emitted[0].category is turbo.ShapeRecursionWarning
    assert result[3] == []


def run_bounded(tmp_path, function, *arguments):
    code = """import importlib.util, sys
spec = importlib.util.spec_from_file_location('warning_cases', sys.argv[1])
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
getattr(module, sys.argv[2])(*sys.argv[3:])
"""
    environment = dict(os.environ)
    environment["COVERAGE_FILE"] = str(
        Path(environment.get("COVERAGE_FILE", tmp_path / ".coverage")).resolve()
    )
    completed = subprocess.run(
        [sys.executable, "-I", "-c", code, str(Path(__file__).resolve()), function, *arguments],
        cwd=tmp_path,
        env=environment,
        capture_output=True,
        text=True,
        timeout=15,
    )
    assert completed.returncode == 0, completed.stdout + completed.stderr


def callback_read_case():
    data = Dataset().parse(data=SOURCE)
    compiled = program()
    observed = []

    def inspect_before_commit(*args, **kwargs):
        observed.append(
            (len(data), snapshot(data), bool(data.query("ASK { <urn:a> <urn:derived> ?o }")))
        )

    before = snapshot(data)
    with warnings.catch_warnings():
        warnings.simplefilter("always", turbo.ShapeRecursionWarning)
        warnings.showwarning = inspect_before_commit
        compiled.validate(data, inplace=True)
    assert observed == [(1, before, False)]
    assert len(data) == 2


def test_warning_callback_can_read_receiver_before_commit_without_deadlock(tmp_path):
    run_bounded(tmp_path, "callback_read_case")


MUTATORS = (
    "load",
    "prefix-load",
    "empty-load",
    "bind",
    "empty-bind",
    "infer",
    "infer-noop",
    "infer-delta",
    "infer-delta-noop",
    "rule-validation",
    "rule-validation-noop",
    "concurrent-load",
)


def callback_mutation_case(mutator):
    from threading import Thread

    data = Dataset().parse(data=SOURCE)
    if mutator in ("infer-noop", "infer-delta-noop"):
        data.infer(inference="owlrl")
    compiled = program()
    inner = program(
        """@prefix sh: <http://www.w3.org/ns/shacl#> .
        <urn:S> sh:targetNode <urn:a>; sh:rule [a sh:TripleRule;
        sh:subject sh:this; sh:predicate <urn:p>; sh:object <urn:b>]."""
        if mutator == "rule-validation-noop"
        else """@prefix sh: <http://www.w3.org/ns/shacl#> .
        <urn:S> sh:targetNode <urn:a>; sh:rule [a sh:TripleRule;
        sh:subject sh:this; sh:predicate <urn:callback>; sh:object <urn:b>]."""
    )
    after_callback = []

    def mutate_receiver(*args, **kwargs):
        if mutator == "load":
            data.parse(data="<urn:callback> <urn:p> <urn:b>.")
        elif mutator == "prefix-load":
            data.parse(data="@prefix fresh: <urn:fresh:> .")
        elif mutator == "empty-load":
            data.parse(data="")
        elif mutator == "bind":
            data._native.bind_namespaces([("fresh", "urn:fresh:")])
        elif mutator == "empty-bind":
            data._native.bind_namespaces([])
        elif mutator.startswith("infer-delta"):
            data.infer_delta(inference="owlrl")
        elif mutator.startswith("infer"):
            data.infer(inference="owlrl")
        elif mutator.startswith("rule-validation"):
            inner.validate(data, inplace=True)
        else:
            assert mutator == "concurrent-load"
            thread = Thread(
                target=lambda: data.parse(data="<urn:concurrent> <urn:p> <urn:b>."), daemon=True
            )
            thread.start()
            thread.join(timeout=3)
            assert not thread.is_alive(), "Dataset remained locked during warning callback"
        after_callback.append(snapshot(data))

    with warnings.catch_warnings():
        warnings.simplefilter("always", turbo.ShapeRecursionWarning)
        warnings.showwarning = mutate_receiver
        with pytest.raises(RuntimeError, match=r"Dataset changed.*warnings"):
            compiled.validate(data, inplace=True)
    assert len(after_callback) == 1
    assert snapshot(data) == after_callback[0]
    assert not bool(data.query("ASK { <urn:a> <urn:derived> ?o }"))


@pytest.mark.parametrize("mutator", MUTATORS)
def test_warning_callback_mutation_conflicts_without_overwriting_callback_writes(tmp_path, mutator):
    run_bounded(tmp_path, "callback_mutation_case", mutator)


def test_nonwarning_rule_commit_does_not_consult_warning_policy(monkeypatch):
    data = Dataset().parse(data=SOURCE)
    compiled = program(SHAPES.replace("sh:condition ex:C;", ""))

    def forbidden(*args, **kwargs):
        raise AssertionError("No-warning validation consulted warning policy")

    monkeypatch.setattr(warnings, "warn", forbidden)
    monkeypatch.setattr(turbo, "ShapeRecursionWarning", None)
    compiled.validate(data, inplace=True)
    assert bool(data.query("ASK { <urn:a> <urn:derived> ?o }"))


def callback_failed_or_independent_case(operation):
    data = Dataset().parse(data=SOURCE)
    if operation in ("invalid-infer", "invalid-delta"):
        data.parse(
            data="""<urn:keyAlias> <http://www.w3.org/2000/01/rdf-schema#subPropertyOf>
          <http://www.w3.org/2002/07/owl#hasKey>.
          <urn:C> <urn:keyAlias> <urn:broken>."""
        )
    compiled = program()
    before = snapshot(data)

    def failed_or_independent_write(*args, **kwargs):
        if operation == "invalid-load":
            with pytest.raises(ValueError):
                data.parse(data="@prefix fresh: <urn:fresh:>. <urn:new> <urn:p> <urn:o>. <")
        elif operation == "invalid-bind":
            with pytest.raises(ValueError):
                data._native.bind_namespaces([("fresh", "urn:fresh:"), ("bad", "relative")])
        elif operation in ("invalid-infer", "invalid-delta"):
            with pytest.raises(ValueError, match="Malformed RDF list"):
                if operation == "invalid-infer":
                    data.infer(inference="owlrl")
                else:
                    data.infer_delta(inference="owlrl")
        elif operation == "invalid-rules":
            inner = program(SHAPES.replace("sh:condition ex:C;", ""))
            with pytest.raises(RuntimeError, match="iteration limit"):
                inner.validate(data, inplace=True, iterate_rules=True, max_rule_iterations=1)
        elif operation == "copy":
            copied = data.copy()
            copied.parse(data="<urn:copy> <urn:p> <urn:o>.")
        else:
            assert operation == "readonly"
            program(SHAPES.replace("sh:condition ex:C;", "")).validate(data)
        assert snapshot(data) == before

    with warnings.catch_warnings():
        warnings.simplefilter("always", turbo.ShapeRecursionWarning)
        warnings.showwarning = failed_or_independent_write
        compiled.validate(data, inplace=True)
    assert bool(data.query("ASK { <urn:a> <urn:derived> ?o }"))


@pytest.mark.parametrize(
    "operation",
    [
        "invalid-load",
        "invalid-bind",
        "invalid-infer",
        "invalid-delta",
        "invalid-rules",
        "copy",
        "readonly",
    ],
)
def test_failed_or_independent_mutation_does_not_conflict(tmp_path, operation):
    run_bounded(tmp_path, "callback_failed_or_independent_case", operation)
