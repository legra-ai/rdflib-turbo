"""Run actual vendored storage unit tests with the project's dependency patches."""

import json
import subprocess
import tomllib
from pathlib import Path

# bounded: both storage profiles require the fixed prefix-scope regressions.
PREFIX_TESTS = (
    "storage::rocksdb_wrapper::prefix_tests::prefix_scan_carry_excludes_adjacent_keys_and_retains_unbounded_extremes",
    "storage::rocksdb_wrapper::prefix_tests::maximum_encoded_prefix_scopes_match_memory_through_rollback_snapshot_and_reopen",
)

# bounded: six graph-role layout, codec and owner-scope contracts in both profiles.
COMPACT_GRAPH_TESTS = (
    "storage::compact_graph_scope_tests::compact_encoded_graph_roles_reduce_actual_quad_layout",
    "storage::compact_graph_scope_tests::compact_graph_scopes_keep_pending_rollback_and_owned_snapshot_identity",
    "storage::compact_graph_scope_tests::compact_graph_scopes_reopen_with_exact_names_and_empty_declarations",
    "storage::compact_graph_codec_tests::compact_graph_names_preserve_every_v1_named_index_key_byte",
    "storage::compact_graph_codec_tests::compact_graph_names_preserve_every_v1_default_index_key_byte",
    "storage::compact_graph_codec_tests::compact_graph_name_decoding_rejects_literal_graph_slots_in_all_indexes",
)

# bounded: the fixed storage contracts must never silently select zero tests.
CHANGE_TESTS = (
    "store::generalized::lexical_tests::union_row_contexts_pin_the_same_version_and_outlive_outer_cursor_and_store",
    "storage::memory::changes_tests::prepared_removal_discard_consumes_ids_without_decoding_or_accepting_additions",
    "store::generalized::lexical_tests::encoded_union_projection_deduplicates_full_terms_on_one_pinned_reader",
    "storage::memory::changes_tests::prepared_capture_keeps_caller_unpublished_and_requires_complete_acceptance",
    "store::generalized::changes_tests::prepared_lexical_changes_stay_unpublished_and_reject_incomplete_or_wrong_owner_acceptance",
    "store::generalized::changes_tests::committed_quad_lexical_projection_preserves_all_graph_name_forms",
    "store::generalized::changes_tests::persistent_capture_refuses_before_writes_and_keeps_reopened_data",
    "storage::memory::changes_tests::committed_full_quad_projection_leases_lexical_words_without_string_copies",
    "store::generalized::changes_tests::normal_commit_cannot_discard_active_capture_or_publish_its_writes",
    "storage::memory::changes_tests::captured_changes_cancel_mutations_and_preserve_first_record_identity",
    "storage::memory::changes_tests::captured_reader_pins_commit_before_later_writer_and_outlives_owner",
    "storage::memory::changes_tests::capture_rejects_prior_writes_and_error_drop_restores_cleanup_state",
    "storage::memory::changes_tests::captured_bulk_cleanup_keeps_exact_graph_kind_and_initial_final_delta",
    "storage::memory::changes_tests::optional_capture_keeps_zero_off_state_and_only_pointer_sized_added_rows",
    "store::generalized::changes_tests::committed_memory_changes_own_full_terms_after_later_write_and_store_drop",
    "store::generalized::changes_tests::memory_change_capture_rejects_prior_writes_and_missing_begin_without_commit",
)

# bounded: only the two native owner cohorts are selected, never unrelated suites.
COHORTS = ("storage::", "store::generalized::")

EXACT_FEATURES = "exact-term-storage,rocksdb,spareval/rdf-set-union,spareval/exact-custom-function-terms,spareval/exact-term-builtins"
UNION_BUDGET = (
    "storage::memory_union_cache_tests::native_union_query_cache_does_not_exceed_sixteen_mebibytes"
)

REQUIRED_TESTS = (
    UNION_BUDGET,
    "storage::memory_union_semantic_tests::native_union_cancellation_errors_when_only_physical_duplicates_remain",
    "storage::memory_union_semantic_tests::native_union_prefix_cancellation_snapshot_and_rollback_preserve_complete_physical_facts",
    *CHANGE_TESTS,
    "store::generalized::lexical_tests::generalized_lexical_projection_keeps_every_role_mask_and_strict_projection_errors",
    "store::generalized::lexical_tests::generalized_projected_words_outlive_owner_and_keep_pinned_committed_version",
    *PREFIX_TESTS,
    *COMPACT_GRAPH_TESTS,
    "storage::memory::lock_tests::quad_range_first_lock_does_not_allocate",
    "storage::generalized_decoder_tests::generalized_strict_decoder_rejects_first_position_before_later_dictionary_reads",
    "storage::generalized_decoder_tests::generalized_decoder_propagates_actual_dictionary_read_failure",
    "storage::generalized_persistent_tests::generalized_persistent_relation_reopens_and_retains_owned_snapshot_after_store_drop",
    "storage::generalized_persistent_tests::generalized_promotion_preserves_existing_v1_persisted_key_bytes",
    "storage::memory::missing_pattern_tests::missing_pattern_stops_probing_indexes_after_first_absent_component",
    "storage::memory::missing_pattern_tests::missing_pattern_reads_follow_commits_rollback_and_retained_snapshots",
    "storage::memory::cardinality_tests::cardinality_snapshot_len_does_not_visit_physical_history",
    "storage::memory::cardinality_tests::cardinality_snapshot_pair_matches_its_visible_rows_during_publication",
    "storage::lexical_hash_tests::lexical_hash_vectors_are_stable",
    "storage::rocksdb::key_hash_tests::fresh_store_declares_its_lexical_hash_scheme",
    "storage::rocksdb::key_hash_tests::mismatched_hash_scheme_fails_for_writers_and_readers",
    "storage::rocksdb::key_hash_tests::missing_hash_scheme_is_not_inferred_from_existing_data",
    "storage::rocksdb::key_hash_tests::hash_scheme_without_storage_version_is_rejected_without_repair",
    "storage::rocksdb::key_hash_tests::undeclared_orphan_data_is_rejected_in_every_column_family",
)


def patch_arguments(root):
    configuration = tomllib.loads((root / "Cargo.toml").read_text())
    # Match CI's warning policy for standalone vendor test crates as well.
    arguments = ["--config", 'build.warnings="deny"']
    for package, patch in configuration["patch"]["crates-io"].items():
        for key, value in patch.items():
            if key == "path":
                value = str((root / value).resolve())
            arguments.extend(["--config", f"patch.crates-io.{package}.{key}={json.dumps(value)}"])
    return arguments


def run_configuration(root, features, required, *, isolated=()):
    cargo = ["cargo", *patch_arguments(root)]
    arguments = [
        "--locked",
        "--manifest-path",
        str(root / "vendor/oxigraph-0.5.13/Cargo.toml"),
        "--no-default-features",
    ]
    if features is not None:
        arguments.extend(["--features", features])
    # Unit builds enable cfg(test), which can hide unused production imports.
    # Build the actual library with the same warning policy before unit checks.
    subprocess.run([*cargo, "build", *arguments, "--lib"], cwd=root, check=True)
    arguments = [*cargo, "test", *arguments, "--lib"]
    listed = subprocess.run(
        [*arguments, "--", "--list"], cwd=root, check=True, stdout=subprocess.PIPE, text=True
    ).stdout
    available = {
        line.removesuffix(": test") for line in listed.splitlines() if line.endswith(": test")
    }
    missing = set((*required, *isolated)) - available
    if missing:
        raise RuntimeError(f"Missing native storage regressions: {sorted(missing)}")
    for cohort in COHORTS:
        if not any(name.startswith(cohort) for name in available):
            raise RuntimeError(f"Empty native owner cohort: {cohort}")
        subprocess.run([*arguments, cohort], cwd=root, check=True)

    for name in isolated:
        result = subprocess.run(
            [*arguments, name, "--", "--ignored", "--exact", "--nocapture", "--test-threads=1"],
            cwd=root,
            check=True,
            stdout=subprocess.PIPE,
            text=True,
        )
        print(result.stdout, end="")
        if "test result: ok. 1 passed; 0 failed;" not in result.stdout:
            raise RuntimeError(
                f"Native isolated regression did not execute exactly one test: {name}"
            )


def run(root):
    run_configuration(root, EXACT_FEATURES, REQUIRED_TESTS, isolated=(UNION_BUDGET,))
    run_configuration(
        root,
        "rocksdb",
        (
            *CHANGE_TESTS,
            *PREFIX_TESTS,
            *COMPACT_GRAPH_TESTS,
            "storage::numeric_encoder::tests::str_hash_stability",
            "storage::rocksdb::upstream_hash_tests::exact_hash_declarations_are_rejected_before_upstream_migration",
        ),
    )


if __name__ == "__main__":
    run(Path(__file__).resolve().parents[1])
