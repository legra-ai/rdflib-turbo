PYTHON ?= 3.13
.PHONY: dev check test build

dev:
	uv venv --python $(PYTHON)
	uv pip install --python .venv/bin/python maturin==1.15.0 'pytest>=8,<10' 'pytest-cov>=7.1,<8' ruff
	VIRTUAL_ENV=$(CURDIR)/.venv .venv/bin/maturin develop --release

check: test
	./scripts/fmt.sh --check
	cargo clippy --locked -- -D warnings
	.venv/bin/ruff check python tests scripts
	.venv/bin/ruff format --check python tests scripts

test:
	.venv/bin/python -m pytest tests -q --cov=rdflib_turbo --cov-branch --cov-fail-under=100

build:
	uv build

.PHONY: benchmark-companions
.PHONY: test-rust-coverage
test-rust-coverage:
	uv run --no-project --python 3.13 python scripts/native_coverage.py --owlrl-source ../owlrl-turbo

benchmark-companions:
	$(MAKE) -C ../pyshacl-turbo benchmark-companions
