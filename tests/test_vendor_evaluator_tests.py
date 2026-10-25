"""Evaluator CI must exercise isolated feature modes and actual patched code."""

import importlib
from pathlib import Path
from types import SimpleNamespace

import pytest


@pytest.fixture
def runner(monkeypatch):
    monkeypatch.syspath_prepend(str(Path(__file__).resolve().parents[1]))
    return importlib.import_module("scripts.vendor_evaluator_tests")


def test_actual_vendor_feature_modes_require_their_semantic_guards(runner, monkeypatch, tmp_path):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        return SimpleNamespace(stdout="\n".join(f"{name}: test" for name in runner.ALL_GUARDS))

    monkeypatch.setattr(runner.subprocess, "run", cargo)
    runner.run(tmp_path)
    assert len(calls) == 6
    for arguments in calls[1::2]:
        assert "--locked" in arguments and "--lib" in arguments
        assert "--no-default-features" in arguments and "--manifest-path" in arguments
        assert "-p" not in arguments
        assert str(tmp_path / "vendor/spareval-0.2.9/Cargo.toml") in arguments
    assert "--features" not in calls[1]
    assert calls[3][-1] == "exact-term-builtins,rdf-set-union"
    assert calls[5][-1] == "exact-term-builtins,rdf-set-union,sparql-12"


def test_missing_mode_specific_guard_fails_before_tests(runner, monkeypatch, tmp_path):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        names = [name for name in runner.ALL_GUARDS if "upstream_terms_default" not in name]
        return SimpleNamespace(stdout="\n".join(f"{name}: test" for name in names))

    monkeypatch.setattr(runner.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing evaluator regressions"):
        runner.run(tmp_path)
    assert len(calls) == 1


def test_explicit_inference_heads_are_mandatory_in_every_feature_profile(runner):
    expected = {
        "inference_construct_stats_tests::inference_construct_explanation_retains_positive_strict_body_work",
        "inference_construct_stats_tests::inference_construct_explanation_keeps_empty_construct_distinct_from_wrong_form",
        "inference_construct_tests::inference_construct_retains_all_roles_without_widening_default_construct",
        "inference_construct_tests::inference_construct_shares_constant_variable_ids_and_rejects_unbound_heads",
        "inference_construct_tests::inference_construct_generated_blank_labels_stay_per_solution_with_literal_predicates",
        "inference_construct_tests::inference_construct_explicitly_refuses_select_ask_and_describe",
        "inference_construct_tests::inference_construct_empty_valid_graph_is_not_a_query_form_error",
        "inference_construct_tests::inference_construct_propagates_source_and_externalization_errors",
        "inference_construct_tests::inference_construct_preserves_strict_row_order_constants_and_generated_node_policy",
    }
    assert expected <= set(runner.GUARDS)
    assert (
        "inference_construct_tests::inference_construct_retains_quoted_roles_and_propagates_nested_template_errors"
        in runner.ALL_GUARDS
    )
