SHELL := /bin/bash
.DEFAULT_GOAL := help

# ═══════════════════════════════════════════════════════════════════════
# IMPORTANT: Always use `make test-python` to run Python tests.
# NEVER run `uv run pytest` or `pytest` directly — maturin develop must
# rebuild the Rust extension first, or you'll test against stale code.
# ═══════════════════════════════════════════════════════════════════════

##@ Setup

.PHONY: setup
setup: ## Install Python dev dependencies (uv sync --group dev).
	uv sync --group dev

.PHONY: install
install: setup ## Build and install pathlibrs in development mode (maturin develop).
	uv run maturin develop

.PHONY: rebuild
rebuild: ## Force full rebuild and reinstall (clean Rust build + develop).
	cargo build
	uv run maturin develop

.PHONY: dev
dev: install ## Alias for install.

##@ Build

.PHONY: build
build: ## Debug build (Rust only, no Python module).
	cargo build

.PHONY: build-release
build-release: ## Release build with LTO enabled.
	cargo build --release

.PHONY: wheel
wheel: setup ## Build release wheel into dist/.
	uv run maturin build --release --out dist

##@ Test

.PHONY: test
test: test-rust test-python ## Run all tests (Rust + Python).

.PHONY: test-rust
test-rust: ## Run Rust unit tests only (fast, no Python).
	cargo test

.PHONY: test-python
test-python: install ## Run Python test suite (smoke tests + vendored CPython tests). Always rebuilds.
	uv run --no-sync pytest tests/ -v

.PHONY: test-windows
test-windows: ## Run Windows-flavour tests on any host OS (for Linux/Mac before pushing).
	uv run python -m pytest tests/ --windows-flavour -v

##@ Format

.PHONY: fmt
fmt: fmt-rust fmt-python ## Format all code (Rust + Python, modifies files).

.PHONY: fmt-rust
fmt-rust: ## Format Rust code (cargo fmt).
	cargo fmt

.PHONY: fmt-python
fmt-python: ## Format Python code (ruff format .).
	uv run --no-sync ruff format --force-exclude .

.PHONY: fmt-check
fmt-check: fmt-check-rust fmt-check-python ## Check formatting without modifying (CI).

.PHONY: fmt-check-rust
fmt-check-rust: ## Check Rust formatting (cargo fmt --check --verbose).
	cargo fmt --check --verbose

.PHONY: fmt-check-python
fmt-check-python: ## Check Python formatting (ruff format --check .).
	uv run --no-sync ruff format --check --force-exclude .

##@ Lint

.PHONY: lint
lint: lint-rust lint-python ## Lint all code (Rust + Python).

.PHONY: lint-rust
lint-rust: ## Rust clippy with warnings as errors.
	cargo clippy --all-targets -- -D warnings

.PHONY: lint-python
lint-python: ## Python ruff check.
	uv run --no-sync ruff check --force-exclude .

##@ CI

.PHONY: check
check: fmt-check lint test ## Run format check + lint + tests — what to run before committing.

.PHONY: ci
ci: fmt-check-rust lint-rust test-rust setup test-python ## Full CI pipeline (same as what runs in GitHub Actions).
	@echo "All CI checks passed."

.PHONY: hooks
hooks: ## Run all pre-commit hooks on all files.
	pre-commit run --all-files

.PHONY: hooks-install
hooks-install: ## Install pre-commit hooks into .git/hooks.
	pre-commit install

.PHONY: clean
clean: ## Remove build artifacts (target/, dist/, caches).
	cargo clean
	rm -rf dist/ build/ .pytest_cache/ __pycache__/ tests/__pycache__/

##@ Benchmarks

BENCH_DIR := benchmarks
BENCH_RESULTS := $(BENCH_DIR)/results
BENCH_BASELINE := $(BENCH_RESULTS)/baseline.json

.PHONY: bench
bench: install ## Run all benchmarks against pathlibrs and built-in pathlib.
	uv run --no-sync pytest $(BENCH_DIR)/ -v --benchmark-only \
		--benchmark-warmup=on \
		--benchmark-columns=min,max,mean,median,stddev,outliers,ops,rounds,iterations

.PHONY: bench-compare
bench-compare: install ## Run benchmarks and compare against saved baseline.
	@test -f $(BENCH_BASELINE) || { echo "No baseline found. Run 'make bench-save' first."; exit 1; }
	uv run --no-sync pytest $(BENCH_DIR)/ -v --benchmark-only \
		--benchmark-warmup=on \
		--benchmark-compare \
		--benchmark-compare-fail=min:10%,mean:10%,median:10% \
		--benchmark-storage=$(BENCH_DIR)/results \
		--benchmark-autosave

.PHONY: bench-save
bench-save: install ## Run benchmarks and save results as baseline for future comparison.
	mkdir -p $(BENCH_DIR)/results
	uv run --no-sync pytest $(BENCH_DIR)/ -v --benchmark-only \
		--benchmark-warmup=on \
		--benchmark-save=baseline \
		--benchmark-storage=$(BENCH_DIR)/results \
		--benchmark-autosave
	@echo "Baseline saved to $(BENCH_BASELINE)"

##@ Help

.PHONY: help
help: ## Show this help message and exit.
	@awk 'BEGIN {FS = ":.*##"; printf "Usage:\n  make \033[36m<target>\033[0m\n"} /^[a-zA-Z_-]+:.*?##/ { printf "  \033[36m%-22s\033[0m %s\n", $$1, $$2 } /^##@/ { printf "\n\033[1m%s\033[0m\n", substr($$0, 5) } ' $(MAKEFILE_LIST)
