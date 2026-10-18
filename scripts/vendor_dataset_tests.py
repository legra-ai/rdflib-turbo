"""Exercise actual patched Dataset units and doctests in both RDF modes."""

import subprocess
from pathlib import Path

from scripts.vendor_memory_tests import patch_arguments

# bounded: critical storage, ownership and ordering regression test names.
REQUIRED_TESTS = (
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
