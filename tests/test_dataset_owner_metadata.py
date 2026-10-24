"""Dataset owner factories preserve explicit namespace and base configuration."""

import pytest
from rdflib_turbo import StoreDataset


@pytest.mark.parametrize("operation", ("copy", "transaction", "transaction-base"))
def test_dataset_copy_and_transaction_keep_namespace_and_base_configuration(operation):
    source = StoreDataset(default_graph_base="urn:original-base:")
    if operation != "transaction-base":
        source.bind("rdf", "urn:explicit-rdf:", replace=True)
    namespaces = dict(source.namespaces())
    if operation == "copy":
        copied = source.copy()
        assert dict(copied.namespaces()) == namespaces
        assert copied.default_graph.base == source.default_graph.base
    else:
        with source.transaction() as pending:
            assert dict(pending.namespaces()) == namespaces
            assert pending.default_graph.base == source.default_graph.base
        assert dict(source.namespaces()) == namespaces
