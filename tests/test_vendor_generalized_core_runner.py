"""The maintained vendor runner executes the full-Term dataset regressions."""

import importlib
import subprocess
from pathlib import Path
from types import SimpleNamespace

import pytest

# bounded: three streaming-removal contracts required in both RDF profiles.
STREAM_REMOVAL = (
    "dataset::generalized_remove_tests::generalized_streaming_removal_preserves_first_arcs_and_current_insertions_on_rollback",
    "dataset::generalized_remove_tests::generalized_streaming_removal_keeps_exact_graph_kinds_and_all_pattern_masks",
    "dataset::generalized_remove_tests::generalized_streaming_removal_absence_keeps_additive_fast_path_and_unwind_restores_words",
)

# bounded: eleven independently named mutation, equality and storage contracts.
CORE = (
    "dataset::generalized_tests::generalized_equality_compares_full_roles_and_graph_kinds_independent_of_insert_order",
    "dataset::mutation_tests::absent_removal_does_not_promote_additive_journal",
    "dataset::mutation_tests::first_state_sets_and_delta_follow_current_presence",
    "dataset::mutation_tests::alias_reinsert_rollback_restores_original_arc_in_all_indexes",
    "dataset::mutation_tests::mutation_unwind_restores_graph_kinds_before_releasing_new_words",
    "dataset::generalized_tests::generalized_storage_preserves_every_role_and_all_pattern_masks",
    "dataset::generalized_tests::generalized_record_layout_hash_order_and_six_shared_indexes",
    "dataset::generalized_tests::generalized_journal_restores_original_incarnations_and_current_delta",
    "dataset::generalized_tests::generalized_unwind_and_commit_preserve_literal_roles_and_dictionary_lifetime",
    "dataset::generalized_tests::six_generalized_orders_and_hashes_match_canonical_term_tuples",
    "dataset::generalized_tests::every_small_mutation_sequence_keeps_first_state_delta_and_undo",
)

# bounded: preserve the twelve preexisting shared-index contracts byte-for-byte.
ORIGINAL = (
    "dataset::shared_record_layout_tests::stored_index_entries_are_one_pointer",
    "dataset::shared_tests::all_ranges_and_mutations_match_exact_quad_reference",
    "dataset::shared_tests::all_six_indexes_and_journal_share_one_immutable_record",
    "dataset::shared_tests::all_six_orders_match_the_original_tuple_orders",
    "dataset::shared_edge_tests::generated_numeric_blank_nodes_work_in_all_ranges_and_rollback",
    "dataset::shared_edge_tests::clone_owns_decode_state_after_source_drop_and_independent_canonicalization",
    "dataset::shared_edge_tests::independent_record_allocations_and_sentinels_keep_each_exact_order",
    "dataset::endpoint_tests::zero_numeric_id_round_trips_exact_identity",
    "dataset::endpoint_tests::maximum_id_subject_range_matches_stored_quad",
    "dataset::scoped_pattern_tests::scoped_patterns_do_not_decode_unrelated_graph_rows",
    "dataset::scoped_pattern_tests::iterator_retains_dataset_terms_after_temporary_patterns_drop",
    "dataset::scoped_pattern_tests::pattern_encodes_each_supplied_component_once_for_every_mask",
)

COMPLETE = STREAM_REMOVAL + ORIGINAL + CORE


@pytest.fixture
def runner(monkeypatch):
    monkeypatch.syspath_prepend(str(Path(__file__).resolve().parents[1]))
    return importlib.import_module("scripts.vendor_dataset_tests")


def test_core_inventory_is_complete_and_both_profiles_execute(tmp_path, monkeypatch, runner):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append((arguments, kwargs))
        return SimpleNamespace(stdout="\n".join(f"{name}: test" for name in COMPLETE))

    monkeypatch.setattr(runner.subprocess, "run", cargo)
    runner.run(tmp_path)
    assert runner.REQUIRED_TESTS == COMPLETE
    assert len(calls) == 4
    for start in (0, 2):
        inventory, execution = [entry[0] for entry in calls[start : start + 2]]
        assert inventory[-2:] == ["--", "--list"]
        assert "--skip" not in execution and "--lib" not in execution
        assert "--locked" in execution
        assert execution[execution.index("--manifest-path") + 1] == str(
            tmp_path / "vendor/oxrdf-0.3.5/Cargo.toml"
        )
        assert 'build.warnings="deny"' in execution
        assert all(kwargs["cwd"] == tmp_path and kwargs["check"] for _, kwargs in calls)
    assert calls[1][0][calls[1][0].index("--features") + 1] == ("rdfc-10,rdf-10-string-identity")
    assert calls[3][0][calls[3][0].index("--features") + 1] == (
        "rdfc-10,rdf-10-string-identity,rdf-12,serde"
    )


@pytest.mark.parametrize("missing", COMPLETE)
@pytest.mark.parametrize("extended", (False, True))
def test_every_missing_core_contract_stops_before_execution(
    tmp_path, monkeypatch, runner, missing, extended
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        hide = not extended or "rdfc-10,rdf-10-string-identity,rdf-12,serde" in arguments
        names = [name for name in COMPLETE if not hide or name != missing]
        return SimpleNamespace(stdout="\n".join(f"{name}: test" for name in names))

    monkeypatch.setattr(runner.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native dataset regressions"):
        runner.run(tmp_path)
    assert len(calls) == (3 if extended else 1)


def test_empty_core_selection_is_a_hard_error(tmp_path, monkeypatch, runner):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        return SimpleNamespace(stdout="0 tests, 0 benchmarks\n")

    monkeypatch.setattr(runner.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native dataset regressions"):
        runner.run(tmp_path)
    assert len(calls) == 1


@pytest.mark.parametrize("phase", ("list", "execute"))
def test_core_failures_are_propagated_without_running_later_phases(
    tmp_path, monkeypatch, runner, phase
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    failure = subprocess.CalledProcessError(17, ["cargo", phase])
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        actual = "list" if "--list" in arguments else "execute"
        if phase == actual:
            raise failure
        return SimpleNamespace(stdout="\n".join(f"{name}: test" for name in COMPLETE))

    monkeypatch.setattr(runner.subprocess, "run", cargo)
    with pytest.raises(subprocess.CalledProcessError) as captured:
        runner.run(tmp_path)
    assert captured.value is failure
    assert len(calls) == {"list": 1, "execute": 2}[phase]
