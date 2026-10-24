"""Binding-only consumers need not initialize unrelated Python graph facades."""

import subprocess
import sys

import pytest
import rdflib_turbo
from rdflib_turbo.graph import Graph, StoreDataset
from rdflib_turbo.store import Store, Transaction, ValidationError


@pytest.mark.parametrize("use_dataset", [False, True])
def test_binding_only_import_is_lazy_and_dataset_loads_canonical_facade(use_dataset):
    source = f"""
import sys
import rdflib_turbo
from rdflib_turbo import _native
assert _native.NativeDataset is not None
assert set(rdflib_turbo.__all__).issubset(dir(rdflib_turbo))
if {use_dataset!r}:
    from rdflib_turbo import Dataset
    data = Dataset().parse(data='<urn:s> <urn:p> <urn:o>.', format='nt')
    assert len(data) == 1
assert ('rdflib_turbo.graph' in sys.modules) is {use_dataset!r}
assert ('rdflib_turbo.store' in sys.modules) is {use_dataset!r}
if {use_dataset!r}:
    assert rdflib_turbo.Dataset is rdflib_turbo.StoreDataset
"""
    result = subprocess.run(
        [sys.executable, "-I", "-c", source], capture_output=True, text=True, check=False
    )
    assert result.returncode == 0, result.stderr


def test_public_exports_keep_actual_class_identity_and_star_import():
    assert set(rdflib_turbo.__all__).issubset(dir(rdflib_turbo))
    expected = {
        "Dataset": rdflib_turbo.Dataset,
        "Graph": Graph,
        "StoreDataset": StoreDataset,
        "Store": Store,
        "Transaction": Transaction,
        "ValidationError": ValidationError,
    }
    exported = {}
    exec("from rdflib_turbo import *", exported)
    for name, cls in expected.items():
        assert getattr(rdflib_turbo, name) is exported[name] is cls
    with pytest.raises(AttributeError, match=r"rdflib_turbo.*no attribute.*missing_export"):
        getattr(rdflib_turbo, "missing_export")
