"""Test actual evaluator code with independently selected feature configurations."""

import subprocess
from pathlib import Path

from scripts.vendor_memory_tests import patch_arguments

GUARDS = (
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
ALL_GUARDS = (*GUARDS, DEFAULT_GUARD, EXACT_GUARD, RDF12_GUARD)


def run(root):
    modes = (
        (None, (DEFAULT_GUARD,)),
        ("exact-term-builtins,rdf-set-union", (EXACT_GUARD,)),
        ("exact-term-builtins,rdf-set-union,sparql-12", (EXACT_GUARD, RDF12_GUARD)),
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
            [*arguments, "--", "--list"], cwd=root, check=True, capture_output=True, text=True
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
