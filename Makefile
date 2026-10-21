PYTHON ?= 3.13
.PHONY: dev check test build

dev:
	uv sync --locked --extra test --python $(PYTHON) --reinstall-package rdflib-turbo

check: test
	./scripts/fmt.sh --check
	cargo clippy --locked -- -D warnings
	uv run --locked --extra test ruff check python tests scripts
	uv run --locked --extra test ruff format --check python tests scripts

test: test-build-metadata
	uv run --locked --extra test pytest tests -q --cov=rdflib_turbo --cov-branch --cov-fail-under=100

.PHONY: test-build-metadata
test-build-metadata:
	cargo test --locked --manifest-path tests/build-metadata/Cargo.toml

build:
	uv build

.PHONY: benchmark-companions
.PHONY: test-rust-coverage
test-rust-coverage:
	uv run --no-project --python 3.13 python scripts/native_coverage.py --owlrl-source ../owlrl-turbo

benchmark-companions:
	$(MAKE) -C ../pyshacl-turbo benchmark-companions

.PHONY: test-memory-storage test-store-contract
test-memory-storage:
	uv run --no-project python scripts/vendor_memory_tests.py

test-store-contract: test-memory-storage
	uv run --no-project python vendor/oxrocksdb-sys-0.5.11/patch-tests/thread_table_identity.py
	cargo test --locked --lib store_tests
	cargo test --locked --manifest-path tests/store-contract/Cargo.toml --no-default-features
	cargo test --locked --manifest-path tests/store-contract/Cargo.toml --features rdf-12,persistence
