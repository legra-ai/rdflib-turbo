"""CI must execute every allocation budget and reject an empty Cargo selection."""

import importlib
from pathlib import Path

import pytest


@pytest.fixture
def budgets(monkeypatch):
    monkeypatch.syspath_prepend(str(Path(__file__).resolve().parents[1]))
    return importlib.import_module("scripts.allocation_budgets")


def test_missing_budget_fails_before_running_tests(budgets, monkeypatch):
    calls = []

    def run(arguments, environment, *, capture=False):
        calls.append(arguments)
        return "another_test: test\n"

    monkeypatch.setattr(budgets, "run", run)
    with pytest.raises(RuntimeError, match="Missing allocation budget"):
        budgets.run_budgets({})
    assert len(calls) == 1


def test_budgets_run_in_separate_processes_with_exact_selection(budgets, monkeypatch):
    calls = []
    environment = {"PYO3_PYTHON": "/selected/python"}

    def run(arguments, received_environment, *, capture=False):
        assert received_environment is environment
        calls.append((arguments, capture))
        return "\n".join(f"{test}: test" for test in budgets.TESTS) if capture else None

    monkeypatch.setattr(budgets, "run", run)
    budgets.run_budgets(environment)
    assert len(calls) == 1 + len(budgets.TESTS)
    for (arguments, capture), test in zip(calls[1:], budgets.TESTS, strict=True):
        assert not capture
        assert arguments[-6:] == [
            test,
            "--",
            "--ignored",
            "--exact",
            "--nocapture",
            "--test-threads=1",
        ]


def test_failed_budget_stops_remaining_processes(budgets, monkeypatch):
    calls = []

    def run(arguments, environment, *, capture=False):
        if capture:
            return "\n".join(f"{test}: test" for test in budgets.TESTS)
        calls.append(arguments)
        raise RuntimeError("budget exceeded")

    monkeypatch.setattr(budgets, "run", run)
    with pytest.raises(RuntimeError, match="budget exceeded"):
        budgets.run_budgets({})
    assert len(calls) == 1


def test_native_boolean_membership_budget_is_required(budgets):
    assert "store::read_input_tests::native_boolean_membership_avoids_result_rows" in budgets.TESTS


def test_borrowed_validation_view_budget_is_required(budgets):
    assert (
        "inference::borrowed_tests::borrowed_validation_view_does_not_clone_large_base_lexical_storage"
        in budgets.TESTS
    )


@pytest.mark.parametrize(
    "name",
    (
        "inference::scoped::boolean_tests::scoped_boolean_probes_do_not_clone_bound_lexical_terms",
        "inference::backend::boolean_tests::default_dataset_boolean_does_not_clone_bound_lexical_terms",
        "inference::backend::boolean_tests::transaction_destination_boolean_does_not_clone_bound_lexical_terms",
        "inference::borrowed::boolean_tests::borrowed_overlay_boolean_does_not_materialize_overlay_lexical_terms",
    ),
)
def test_inference_boolean_allocation_budgets_are_required(budgets, name):
    assert name in budgets.TESTS


@pytest.mark.parametrize(
    "name",
    (
        "store::pattern::count_tests::union_count_does_not_allocate_decoded_lexical_payloads",
        "store::pattern::count_tests::transaction_union_count_does_not_allocate_decoded_lexical_payloads",
    ),
)
def test_union_count_allocation_budgets_are_required(budgets, name):
    assert name in budgets.TESTS


def test_bare_allocation_entrypoint_uses_verified_locked_runtime(budgets, monkeypatch, tmp_path):
    from contextlib import contextmanager

    authority = tmp_path / "yaml-inputs.json"
    authority.write_text("authority")
    selected = {"PYO3_PYTHON": "/isolated/project/.venv/bin/python"}
    calls = []

    @contextmanager
    def locked(root, output, python, yaml_authority):
        assert root == Path(budgets.__file__).resolve().parents[1]
        assert yaml_authority == authority
        calls.append("enter")
        yield selected
        calls.append("verify-after")

    def bare_forbidden(*args, **kwargs):
        raise AssertionError("Bare allocation interpreter cannot supply embedded dependencies")

    monkeypatch.setattr(budgets, "locked_python_environment", locked, raising=False)
    monkeypatch.setattr(budgets, "embedded_python_environment", bare_forbidden, raising=False)
    monkeypatch.setattr(budgets, "run_budgets", lambda environment: calls.append(environment))
    monkeypatch.setattr(
        budgets.sys, "argv", ["allocation_budgets", "--yaml-authority", str(authority)]
    )
    budgets.main()
    assert calls == ["enter", selected, "verify-after"]


@pytest.mark.parametrize("malformed_receipt", [False, True])
def test_memory_profile_entrypoint_uses_same_verified_locked_runtime(
    monkeypatch, tmp_path, malformed_receipt
):
    import importlib.util
    import json
    from contextlib import contextmanager

    root = Path(__file__).resolve().parents[1]
    monkeypatch.syspath_prepend(str(root / "scripts"))
    spec = importlib.util.spec_from_file_location(
        "profile_memory_driver", root / "scripts/profile_memory.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    data, shapes, authority = (tmp_path / name for name in ("data.ttl", "shapes.ttl", "yaml.json"))
    for path in (data, shapes, authority):
        path.write_text("frozen")
    selected = {"PYO3_PYTHON": "/isolated/project/.venv/bin/python"}
    calls = []

    @contextmanager
    def locked(owner, output, python, yaml_authority):
        assert owner == root and yaml_authority == authority
        calls.append("enter")
        yield selected
        calls.append("verify-after")

    def run(arguments, environment=None, *, capture=False):
        if arguments[0] != "cargo":
            return "revision\n" if "rev-parse" in arguments else ""
        assert environment["PYO3_PYTHON"] == selected["PYO3_PYTHON"]
        assert arguments[arguments.index("--package") + 1] == "rdflib-turbo-native"
        calls.append("profile")
        result = "\n".join(
            "MEMORY_PHASE " + json.dumps({"phase": phase})
            for phase in (
                "setup",
                "parse",
                "inference",
                "compile_shapes",
                "validation_native_report",
                "released",
            )
        )
        return result + ('\nMEMORY_SEMANTICS {"guard":true}unexpected' if malformed_receipt else "")

    monkeypatch.setattr(module, "locked_python_environment", locked, raising=False)
    monkeypatch.setattr(module, "run", run)
    monkeypatch.setattr(
        module.sys,
        "argv",
        [
            "profile_memory",
            "--owner",
            "generalized-dataset",
            "--data",
            str(data),
            "--shapes",
            str(shapes),
            "--output",
            str(tmp_path / "report.json"),
            "--yaml-authority",
            str(authority),
        ],
    )
    if malformed_receipt:
        with pytest.raises(json.JSONDecodeError):
            module.main()
        assert (
            (tmp_path / "report.log")
            .read_text()
            .endswith('MEMORY_SEMANTICS {"guard":true}unexpected')
        )
    else:
        module.main()
    assert calls == ["enter", "profile", "verify-after"]
