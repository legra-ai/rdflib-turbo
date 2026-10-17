PYTHON ?= 3.13
.PHONY: dev check test build

dev:
	uv sync --locked --extra test --python $(PYTHON) --reinstall-package rdflib-turbo

check: test
	./scripts/fmt.sh --check
	cargo clippy --locked -- -D warnings
	uv run --locked --extra test ruff check python tests scripts
	uv run --locked --extra test ruff format --check python tests scripts

test:
	uv run --locked --extra test pytest tests -q --cov=rdflib_turbo --cov-branch --cov-fail-under=100

build:
	uv build

.PHONY: benchmark-companions
.PHONY: test-rust-coverage
test-rust-coverage:
	uv run --no-project --python 3.13 python scripts/native_coverage.py --owlrl-source ../owlrl-turbo

benchmark-companions:
	$(MAKE) -C ../pyshacl-turbo benchmark-companions

.PHONY: test-store-contract
test-store-contract:
	cargo test --locked --lib store_tests
	cargo test --locked --manifest-path tests/store-contract/Cargo.toml --no-default-features
	cargo test --locked --manifest-path tests/store-contract/Cargo.toml --features rdf-12,persistence
