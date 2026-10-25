"""Test actual evaluator code with independently selected feature configurations."""

import subprocess
from pathlib import Path

from scripts.vendor_memory_tests import patch_arguments

GENERALIZED_GUARDS = (
    "dataset::generalized_tests::generalized_adapter_retains_all_roles_masks_and_exact_graph_kinds",
    "dataset::generalized_tests::generalized_adapter_borrows_long_terms_after_temporary_patterns_drop",
    "dataset::generalized_tests::generalized_adapter_graph_names_borrow_and_reject_non_graph_terms",
    "dataset::generalized_tests::generalized_select_ask_join_full_terms_while_construct_stays_strict",
    "dataset::generalized_query_tests::generalized_scoped_callbacks_keep_common_rare_and_graph_term_roles",
    "dataset::generalized_query_tests::generalized_source_errors_are_not_empty_results",
    "dataset::generalized_query_tests::generalized_construct_constants_and_generated_blanks_do_not_internalize_new_nodes",
)

GUARDS = (
    *GENERALIZED_GUARDS,
    "inference_construct_stats_tests::inference_construct_explanation_retains_positive_strict_body_work",
    "inference_construct_stats_tests::inference_construct_explanation_keeps_empty_construct_distinct_from_wrong_form",
    "inference_construct_tests::inference_construct_retains_all_roles_without_widening_default_construct",
    "inference_construct_tests::inference_construct_shares_constant_variable_ids_and_rejects_unbound_heads",
    "inference_construct_tests::inference_construct_generated_blank_labels_stay_per_solution_with_literal_predicates",
    "inference_construct_tests::inference_construct_explicitly_refuses_select_ask_and_describe",
    "inference_construct_tests::inference_construct_empty_valid_graph_is_not_a_query_form_error",
    "inference_construct_tests::inference_construct_propagates_source_and_externalization_errors",
    "inference_construct_tests::inference_construct_preserves_strict_row_order_constants_and_generated_node_policy",
    "construct_cache::tests::compact_result_identities_halve_actual_requested_key_storage",
    "construct_cache::tests::construct_cache_preserves_all_positions_and_clear_reuse",
    "construct_cache::tests::wide_result_identities_never_alias_small_keys_in_any_position",
    "construct_tests::construct_internal_dedup_keeps_exact_literal_identity_and_mixed_templates",
    "construct_tests::construct_internal_dedup_rejects_invalid_positions_and_unbound_values",
    "construct_tests::construct_internal_dedup_keeps_template_blank_nodes_per_solution",
    "construct_tests::construct_constants_and_generated_blanks_do_not_require_internalization",
    "dataset::scope_tests::graph_modes_and_all_term_masks_preserve_exact_results",
    "dataset::scope_tests::owned_call_patterns_can_drop_before_results_are_consumed",
    "dataset::scope_tests::invalid_statement_positions_stay_empty",
)
DEFAULT_GUARD = "tests::custom_function_upstream_terms_default"
EXACT_GUARD = "tests::custom_function_exact_terms_opt_in"
RDF12_GUARD = "construct_tests::construct_quoted_triples_keep_nested_and_directional_identity"
# bounded: the fixed critical regression inventory, independent of input RDF size.
GENERALIZED_RDF12_GUARD = "dataset::generalized_query_tests::generalized_quoted_roles_and_directional_literals_remain_borrowed"
INFERENCE_RDF12_GUARD = "inference_construct_tests::inference_construct_retains_quoted_roles_and_propagates_nested_template_errors"
ALL_GUARDS = (
    *GUARDS,
    DEFAULT_GUARD,
    EXACT_GUARD,
    RDF12_GUARD,
    GENERALIZED_RDF12_GUARD,
    INFERENCE_RDF12_GUARD,
)


def run(root):
    modes = (
        (None, (DEFAULT_GUARD,)),
        ("exact-term-builtins,rdf-set-union", (EXACT_GUARD,)),
        (
            "exact-term-builtins,rdf-set-union,sparql-12",
            (EXACT_GUARD, RDF12_GUARD, GENERALIZED_RDF12_GUARD, INFERENCE_RDF12_GUARD),
        ),
    )
    for features, guards in modes:
        arguments = [
            "cargo",
            *patch_arguments(root),
            "test",
            "--locked",
            "--manifest-path",
            str(root / "vendor/spareval-0.2.7/Cargo.toml"),
            "--no-default-features",
            "--lib",
        ]
        if features:
            arguments.extend(["--features", features])
        listing = subprocess.run(
            [*arguments, "--", "--list"], cwd=root, check=True, stdout=subprocess.PIPE, text=True
        ).stdout
        available = {
            line.removesuffix(": test") for line in listing.splitlines() if line.endswith(": test")
        }
        missing = set((*GUARDS, *guards)) - available
        if missing:
            raise RuntimeError(f"Missing evaluator regressions: {sorted(missing)}")
        subprocess.run(arguments, cwd=root, check=True)


if __name__ == "__main__":
    run(Path(__file__).resolve().parents[1])
