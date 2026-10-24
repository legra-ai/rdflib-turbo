"""Exercise actual patched Dataset units and doctests in both RDF modes."""

import subprocess
from pathlib import Path

from scripts.vendor_memory_tests import patch_arguments

# bounded: critical storage, ownership and ordering regression test names.
REQUIRED_TESTS = (
    "dataset::generalized_remove_tests::generalized_streaming_removal_preserves_first_arcs_and_current_insertions_on_rollback",
    "dataset::generalized_remove_tests::generalized_streaming_removal_keeps_exact_graph_kinds_and_all_pattern_masks",
    "dataset::generalized_remove_tests::generalized_streaming_removal_absence_keeps_additive_fast_path_and_unwind_restores_words",
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


def run(root):
    for features in (
        "rdfc-10,rdf-10-string-identity",
        "rdfc-10,rdf-10-string-identity,rdf-12,serde",
    ):
        arguments = [
            "cargo",
            *patch_arguments(root),
            "test",
            "--locked",
            "--manifest-path",
            str(root / "vendor/oxrdf-0.3.4/Cargo.toml"),
            "--features",
            features,
        ]
        listing = subprocess.run(
            [*arguments, "--", "--list"], cwd=root, check=True, stdout=subprocess.PIPE, text=True
        ).stdout
        available = {
            line.removesuffix(": test") for line in listing.splitlines() if line.endswith(": test")
        }
        missing = set(REQUIRED_TESTS) - available
        if missing:
            raise RuntimeError(f"Missing native dataset regressions: {sorted(missing)}")
        subprocess.run(arguments, cwd=root, check=True)


if __name__ == "__main__":
    run(Path(__file__).resolve().parents[1])
