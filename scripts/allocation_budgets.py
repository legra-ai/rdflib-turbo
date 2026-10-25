"""Run every native allocation budget alone and reject missing Cargo test filters."""

import os
import sys

from .native_coverage import embedded_python_environment, run

# bounded: these twenty-one regression budgets each require an isolated test process.
TESTS = (
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
    environment = embedded_python_environment(sys.executable, dict(os.environ))
    environment["PYO3_PYTHON"] = sys.executable
    run_budgets(environment)


if __name__ == "__main__":
    main()
