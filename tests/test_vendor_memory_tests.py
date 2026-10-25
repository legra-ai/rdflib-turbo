"""The native memory unit runner uses the actual vendor library and project patches."""

import importlib
import json
import subprocess
import tomllib
from pathlib import Path
from types import SimpleNamespace

import pytest


@pytest.mark.parametrize("crate", ["oxrdf-0.3.5", "oxigraph-0.5.13", "spareval-0.2.9"])
def test_vendor_lockfiles_resolve_current_local_patches(crate, vendor_memory_tests):
    root = Path(__file__).resolve().parents[1]
    patches = tomllib.loads((root / "Cargo.toml").read_text())["patch"]["crates-io"]
    lock = tomllib.loads((root / "vendor" / crate / "Cargo.lock").read_text())
    packages = lock["package"] + lock.get("patch", {}).get("unused", [])
    for name, patch in patches.items():
        manifest = (root / patch["path"] / "Cargo.toml").resolve()
        version = tomllib.loads(manifest.read_text())["package"]["version"]
        selected = [item for item in packages if item["name"] == name]
        assert len(selected) == 1, name
        assert selected[0]["version"] == version, name
        assert "source" not in selected[0], name
    arguments = [
        "cargo",
        *vendor_memory_tests.patch_arguments(root),
        "metadata",
        "--locked",
        "--manifest-path",
        str(root / "vendor" / crate / "Cargo.toml"),
        "--format-version=1",
        "--no-default-features",
    ]
    if crate == "oxigraph-0.5.13":
        arguments.extend(["--features", "exact-term-storage,rocksdb"])
    metadata = json.loads(
        subprocess.run(arguments, cwd=root, check=True, capture_output=True).stdout
    )
    for name, patch in patches.items():
        selected = [item for item in metadata["packages"] if item["name"] == name]
        if selected:
            assert len(selected) == 1, name
            assert selected[0]["source"] is None, name
            assert (
                Path(selected[0]["manifest_path"])
                == (root / patch["path"] / "Cargo.toml").resolve()
            )
    if crate == "oxigraph-0.5.13":
        required = {"oxrdf", "oxsdatatypes", "spargebra", "sparopt", "spareval", "oxrocksdb-sys"}
        assert required <= {item["name"] for item in metadata["packages"]}


@pytest.fixture
def vendor_memory_tests(monkeypatch):
    monkeypatch.syspath_prepend(str(Path(__file__).resolve().parents[1]))
    return importlib.import_module("scripts.vendor_memory_tests")


def test_vendor_memory_runner_applies_current_root_patches_and_locked_native_tests(
    tmp_path, monkeypatch, vendor_memory_tests
):
    root = tmp_path / "project with spaces"
    root.mkdir()
    (root / "Cargo.toml").write_text(
        '[patch.crates-io]\noxrdf = { path = "vendor/custom-rdf" }\n'
        'fancy-regex = { git = "https://example.org/engine", rev = "abc123" }\n'
    )
    calls = []

    def cargo(arguments, *, cwd, check, **kwargs):
        calls.append((arguments, cwd, check))
        if "--ignored" in arguments:
            return SimpleNamespace(stdout="test result: ok. 1 passed; 0 failed;")
        return SimpleNamespace(
            stdout="\n".join(
                f"{name}: test"
                for name in (
                    *vendor_memory_tests.REQUIRED_TESTS,
                    "storage::numeric_encoder::tests::str_hash_stability",
                    "storage::rocksdb::upstream_hash_tests::exact_hash_declarations_are_rejected_before_upstream_migration",
                )
            )
        )

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    vendor_memory_tests.run(root)
    assert len(calls) == 9
    assert "build" in calls[0][0] and calls[0][0][-1] == "--lib"
    assert calls[1][0][-2:] == ["--", "--list"]
    arguments, directory, checked = calls[2]
    assert directory == root and checked is True
    assert arguments[0] == "cargo"
    assert "--locked" in arguments and "--lib" in arguments
    assert "storage::" in arguments
    assert arguments[arguments.index("--features") + 1] == vendor_memory_tests.EXACT_FEATURES
    assert calls[8][0][calls[8][0].index("--features") + 1] == "rocksdb"
    assert arguments[arguments.index("--manifest-path") + 1] == str(
        root / "vendor/oxigraph-0.5.13/Cargo.toml"
    )
    config = [arguments[i + 1] for i, argument in enumerate(arguments) if argument == "--config"]
    assert f"patch.crates-io.oxrdf.path={json.dumps(str(root / 'vendor/custom-rdf'))}" in config
    assert 'patch.crates-io.fancy-regex.git="https://example.org/engine"' in config
    assert 'patch.crates-io.fancy-regex.rev="abc123"' in config


def test_native_inventory_respects_real_filtering_and_runs_both_owning_cohorts(
    tmp_path, monkeypatch, vendor_memory_tests
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    executed = []
    names = set(vendor_memory_tests.REQUIRED_TESTS) | {
        "storage::numeric_encoder::tests::str_hash_stability",
        "storage::rocksdb::upstream_hash_tests::exact_hash_declarations_are_rejected_before_upstream_migration",
    }

    def cargo(arguments, **kwargs):
        if "--list" in arguments:
            between = arguments[arguments.index("--lib") + 1 : arguments.index("--")]
            selected = {name for name in names if not between or between[0] in name}
            return SimpleNamespace(stdout="\n".join(f"{name}: test" for name in selected))
        if "test" in arguments:
            executed.append(arguments[arguments.index("--lib") + 1])
        return SimpleNamespace(stdout="test result: ok. 1 passed; 0 failed;")

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    vendor_memory_tests.run(tmp_path)
    assert executed == [
        "storage::",
        "store::generalized::",
        vendor_memory_tests.UNION_BUDGET,
        "storage::",
        "store::generalized::",
    ]


def test_vendor_memory_runner_preserves_native_failure(tmp_path, monkeypatch, vendor_memory_tests):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    failure = subprocess.CalledProcessError(7, ["cargo"])

    def cargo(*args, **kwargs):
        raise failure

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(subprocess.CalledProcessError) as captured:
        vendor_memory_tests.run(tmp_path)
    assert captured.value is failure


def test_production_library_failure_stops_before_unit_test_inventory(
    tmp_path, monkeypatch, vendor_memory_tests
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    failure = subprocess.CalledProcessError(9, ["cargo", "build"])
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        if "build" in arguments:
            raise failure
        return SimpleNamespace(
            stdout="\n".join(
                f"{name}: test"
                for name in (
                    *vendor_memory_tests.REQUIRED_TESTS,
                    "storage::numeric_encoder::tests::str_hash_stability",
                    "storage::rocksdb::upstream_hash_tests::exact_hash_declarations_are_rejected_before_upstream_migration",
                )
            )
        )

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(subprocess.CalledProcessError) as captured:
        vendor_memory_tests.run(tmp_path)
    assert captured.value is failure
    assert len(calls) == 1
    assert "--locked" in calls[0] and "--lib" in calls[0]
    assert 'build.warnings="deny"' in calls[0]


def test_missing_instrumented_regressions_fail_before_execution(
    tmp_path, monkeypatch, vendor_memory_tests
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        if "--ignored" in arguments:
            return SimpleNamespace(stdout="test result: ok. 1 passed; 0 failed;")
        return SimpleNamespace(stdout="storage::memory::tests::test_range: test\n")

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native storage regressions"):
        vendor_memory_tests.run(tmp_path)
    assert len(calls) == 2


def test_missing_upstream_hash_regression_fails_inventory(
    tmp_path, monkeypatch, vendor_memory_tests
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        if "--ignored" in arguments:
            return SimpleNamespace(stdout="test result: ok. 1 passed; 0 failed;")
        return SimpleNamespace(
            stdout="\n".join(f"{name}: test" for name in vendor_memory_tests.REQUIRED_TESTS)
        )

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native storage regressions"):
        vendor_memory_tests.run(tmp_path)


def test_missing_lexical_hash_regression_fails_inventory(
    tmp_path, monkeypatch, vendor_memory_tests
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        if "--ignored" in arguments:
            return SimpleNamespace(stdout="test result: ok. 1 passed; 0 failed;")
        return SimpleNamespace(
            stdout="\n".join(
                f"{name}: test"
                for name in vendor_memory_tests.REQUIRED_TESTS
                if "lexical_hash" not in name
            )
        )

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native storage regressions"):
        vendor_memory_tests.run(tmp_path)
    assert len(calls) == 2


def test_missing_upstream_store_guard_fails_inventory(tmp_path, monkeypatch, vendor_memory_tests):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        if "--ignored" in arguments:
            return SimpleNamespace(stdout="test result: ok. 1 passed; 0 failed;")
        return SimpleNamespace(
            stdout="\n".join(
                f"{name}: test"
                for name in (
                    *vendor_memory_tests.REQUIRED_TESTS,
                    "storage::numeric_encoder::tests::str_hash_stability",
                )
            )
        )

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native storage regressions"):
        vendor_memory_tests.run(tmp_path)
    assert len(calls) == 7


@pytest.mark.parametrize(
    "missing",
    (
        "storage::generalized_decoder_tests::generalized_strict_decoder_rejects_first_position_before_later_dictionary_reads",
        "storage::generalized_decoder_tests::generalized_decoder_propagates_actual_dictionary_read_failure",
        "storage::generalized_persistent_tests::generalized_persistent_relation_reopens_and_retains_owned_snapshot_after_store_drop",
        "storage::generalized_persistent_tests::generalized_promotion_preserves_existing_v1_persisted_key_bytes",
    ),
)
def test_missing_generalized_storage_contract_fails_before_execution(
    tmp_path, monkeypatch, vendor_memory_tests, missing
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        if "--ignored" in arguments:
            return SimpleNamespace(stdout="test result: ok. 1 passed; 0 failed;")
        names = (
            *vendor_memory_tests.REQUIRED_TESTS,
            "storage::numeric_encoder::tests::str_hash_stability",
            "storage::rocksdb::upstream_hash_tests::exact_hash_declarations_are_rejected_before_upstream_migration",
        )
        return SimpleNamespace(
            stdout="\n".join(f"{name}: test" for name in names if name != missing)
        )

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native storage regressions"):
        vendor_memory_tests.run(tmp_path)
    assert len(calls) == 2


@pytest.mark.parametrize(
    "missing",
    (
        "storage::rocksdb_wrapper::prefix_tests::prefix_scan_carry_excludes_adjacent_keys_and_retains_unbounded_extremes",
        "storage::rocksdb_wrapper::prefix_tests::maximum_encoded_prefix_scopes_match_memory_through_rollback_snapshot_and_reopen",
    ),
)
def test_missing_prefix_scope_contract_fails_before_execution(
    tmp_path, monkeypatch, vendor_memory_tests, missing
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        if "--ignored" in arguments:
            return SimpleNamespace(stdout="test result: ok. 1 passed; 0 failed;")
        names = (
            *vendor_memory_tests.REQUIRED_TESTS,
            "storage::numeric_encoder::tests::str_hash_stability",
            "storage::rocksdb::upstream_hash_tests::exact_hash_declarations_are_rejected_before_upstream_migration",
        )
        return SimpleNamespace(
            stdout="\n".join(f"{name}: test" for name in names if name != missing)
        )

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native storage regressions"):
        vendor_memory_tests.run(tmp_path)
    assert len(calls) == 2


@pytest.mark.parametrize("upstream", (False, True))
def test_missing_compact_graph_contract_fails_before_execution(
    tmp_path, monkeypatch, vendor_memory_tests, upstream
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []
    missing = "storage::compact_graph_codec_tests::compact_graph_name_decoding_rejects_literal_graph_slots_in_all_indexes"

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        if "--ignored" in arguments:
            return SimpleNamespace(stdout="test result: ok. 1 passed; 0 failed;")
        names = (
            *vendor_memory_tests.REQUIRED_TESTS,
            "storage::numeric_encoder::tests::str_hash_stability",
            "storage::rocksdb::upstream_hash_tests::exact_hash_declarations_are_rejected_before_upstream_migration",
        )
        hide = not upstream or vendor_memory_tests.EXACT_FEATURES not in arguments
        return SimpleNamespace(
            stdout="\n".join(f"{name}: test" for name in names if not hide or name != missing)
        )

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    with pytest.raises(RuntimeError, match="Missing native storage regressions"):
        vendor_memory_tests.run(tmp_path)
    assert len(calls) == (7 if upstream else 2)


@pytest.mark.parametrize(
    "name",
    (
        "generalized_lexical_projection_keeps_every_role_mask_and_strict_projection_errors",
        "generalized_projected_words_outlive_owner_and_keep_pinned_committed_version",
    ),
)
def test_full_term_lexical_projection_is_a_mandatory_owner_guard(vendor_memory_tests, name):
    assert f"store::generalized::lexical_tests::{name}" in vendor_memory_tests.REQUIRED_TESTS


@pytest.mark.parametrize(
    "summary", ("test result: ok. 0 passed; 0 failed;", "test result: ok. 1 passed; 0 failed;")
)
def test_union_budget_requires_exact_isolated_execution(
    tmp_path, monkeypatch, vendor_memory_tests, summary
):
    (tmp_path / "Cargo.toml").write_text("[patch.crates-io]\n")
    calls = []
    name = "storage::memory::lock_tests::union_cache::native_union_query_cache_does_not_exceed_sixteen_mebibytes"

    def cargo(arguments, **kwargs):
        calls.append(arguments)
        if "--list" in arguments:
            return SimpleNamespace(
                stdout="\n".join(
                    f"{item}: test" for item in (*vendor_memory_tests.REQUIRED_TESTS, name)
                )
            )
        return SimpleNamespace(stdout=summary)

    monkeypatch.setattr(vendor_memory_tests.subprocess, "run", cargo)
    if "0 passed" in summary:
        with pytest.raises(RuntimeError, match="did not execute exactly one"):
            vendor_memory_tests.run_configuration(
                tmp_path,
                vendor_memory_tests.EXACT_FEATURES,
                vendor_memory_tests.REQUIRED_TESTS,
                isolated=(name,),
            )
    else:
        vendor_memory_tests.run_configuration(
            tmp_path,
            vendor_memory_tests.EXACT_FEATURES,
            vendor_memory_tests.REQUIRED_TESTS,
            isolated=(name,),
        )
    assert calls[-1][-6:] == [name, "--", "--ignored", "--exact", "--nocapture", "--test-threads=1"]
    assert "spareval/rdf-set-union" in vendor_memory_tests.EXACT_FEATURES


def test_union_semantics_and_budget_are_mandatory(vendor_memory_tests):
    required = set(vendor_memory_tests.REQUIRED_TESTS)
    assert (
        "storage::memory::lock_tests::union_cache::semantics::native_union_cancellation_errors_when_only_physical_duplicates_remain"
        in required
    )
    assert vendor_memory_tests.UNION_BUDGET in required


def test_native_union_required_names_match_actual_compiled_inventory(vendor_memory_tests):
    fixture = Path(__file__).parent / "fixtures/native_union_test_inventory.json"
    actual = json.loads(fixture.read_text())
    assert set(vendor_memory_tests.REQUIRED_TESTS) <= set(actual["native_exact"])
    assert vendor_memory_tests.UNION_BUDGET in actual["native_exact"]
    assert vendor_memory_tests.UNION_BUDGET not in actual["native_standard"]
