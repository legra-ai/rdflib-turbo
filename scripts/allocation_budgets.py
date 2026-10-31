"""Run every native allocation budget alone and reject missing Cargo test filters."""

import argparse
import sys
import uuid
from pathlib import Path

from .native_coverage import locked_python_environment, run

# bounded: this fixed regression inventory requires one isolated process per budget.
TESTS = (
    "allocation_tests::focus_order::focus_identifier_ordering_has_linear_payload_allocation",
    "allocation_tests::focus_order::targeting_index_shape_order_has_linear_payload_allocation",
    "allocation_tests::focus_order::focus_literal_ordering_has_linear_payload_allocation",
    "allocation_tests::focus_order::focus_literal_ordering_has_bounded_scratch_peak",
    "allocation_tests::current_thread::direct_scope_isolation_controls",
    "allocation_tests::report_overlap::report_render_does_not_retain_target_cache_width",
    "allocation_tests::inference_heap_budget",
    "inference::pending_retirement_tests::consumed_generalized_frontier_releases_storage_before_iteration_finishes",
    "allocation_tests::named_inference_does_not_clone_source_lexical_storage",
    "inference::staged::allocation_tests::logical_membership_borrows_source_lexical",
    "inference::scoped::boolean_tests::scoped_boolean_probes_do_not_clone_bound_lexical_terms",
    "inference::backend::boolean_tests::default_dataset_boolean_does_not_clone_bound_lexical_terms",
    "inference::backend::boolean_tests::transaction_destination_boolean_does_not_clone_bound_lexical_terms",
    "inference::borrowed::boolean_tests::borrowed_overlay_boolean_does_not_materialize_overlay_lexical_terms",
    "inference::borrowed_tests::borrowed_validation_view_does_not_clone_large_base_lexical_storage",
    "allocation_tests::shared_index_retained_budget",
    "allocation_tests::scoped_query_binding_does_not_copy_graph_lexical",
    "allocation_tests::parse_heap_budget",
    "allocation_tests::failed_parse_releases_staged_terms",
    "store::pattern::count_tests::scoped_count_does_not_allocate_decoded_lexical_payloads",
    "store::pattern::count_tests::union_count_does_not_allocate_decoded_lexical_payloads",
    "store::pattern::count_tests::transaction_union_count_does_not_allocate_decoded_lexical_payloads",
    "store::read::tests::leases::scoped_lexical_scan_does_not_copy_stored_payloads",
    "store::read_input_tests::native_read_filter_binding_does_not_copy_large_input_lexical",
    "store::read::tests::exact_membership_allocates_only_one_result_row",
    "store::read_input_tests::native_boolean_membership_avoids_result_rows",
    "store::read_input_tests::native_raw_membership_avoids_rows_and_lexical_copies",
    "allocation_tests::report_retirement::direct_select_report_retires_owned_messages",
    "allocation_tests::report_retirement::nested_select_report_retires_owned_messages",
)
COMMAND = ["cargo", "test", "--locked", "--release", "--features", "allocation-profiling"]


def run_budgets(environment):
    listing = run([*COMMAND, "--", "--list"], environment, capture=True)
    # bounded: the compiled native unit-test inventory, independent of input RDF size.
    available = {
        line.removesuffix(": test") for line in listing.splitlines() if line.endswith(": test")
    }
    missing = set(TESTS) - available
    if missing:
        raise RuntimeError(f"Missing allocation budget tests: {sorted(missing)}")
    for test in TESTS:
        run(
            [*COMMAND, test, "--", "--ignored", "--exact", "--nocapture", "--test-threads=1"],
            environment,
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--yaml-authority", type=Path, required=True)
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[1]
    output = root / "target/allocation-budgets" / uuid.uuid4().hex
    with locked_python_environment(
        root, output, sys.executable, args.yaml_authority
    ) as environment:
        run_budgets(environment)


if __name__ == "__main__":
    main()
