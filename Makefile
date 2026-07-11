.PHONY: build build-release dev install setup test test-rust test-python \
        fmt fmt-check fmt-rust fmt-python lint lint-rust lint-python \
        clean wheel ci

# Build debug binary (Rust only, no Python module)
build:
	cargo build

# Build with release optimizations (LTO)
build-release:
	cargo build --release

# Install Python dev dependencies (uv sync)
setup:
	uv sync --group dev

# Install in development mode (build + install into current Python env)
dev: install

install: setup
	uv run maturin develop

# Run all tests (Rust + Python)
test: test-rust test-python

# Rust unit tests only (fast, no Python)
test-rust:
	cargo test

# Python test suite (smoke tests + vendored CPython tests)
test-python:
	uv run pytest tests/ -v

# Format all code
fmt: fmt-rust fmt-python

# Format Rust code
fmt-rust:
	cargo fmt

# Format Python code
fmt-python:
	uv run ruff format .

# Check formatting without modifying (CI)
fmt-check: fmt-check-rust fmt-check-python

fmt-check-rust:
	cargo fmt --check --verbose

fmt-check-python:
	uv run ruff format --check .

# Lint all code
lint: lint-rust lint-python

# Rust clippy with warnings as errors
lint-rust:
	cargo clippy --all-targets -- -D warnings

# Python ruff lint
lint-python:
	uv run ruff check .

# Run all checks (format + lint + tests) — same as CI
check: fmt-check lint test
	@echo "All checks passed."

# Build release wheel
wheel: setup
	uv run maturin build --release --out dist

# Clean build artifacts
clean:
	cargo clean
	rm -rf dist/ build/ .pytest_cache/ __pycache__/ tests/__pycache__/

# Full CI pipeline (format, clippy, Rust tests, Python tests)
ci: fmt-check-rust lint-rust test-rust setup test-python
	@echo "All CI checks passed."
