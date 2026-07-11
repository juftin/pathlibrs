# AGENTS.md — pathlibrs

## Project Overview

`pathlibrs` is a fast pure-Rust implementation of Python's `pathlib`, shipped as a PyO3 native extension. It targets the CPython 3.14 `pathlib` API surface with single-binary `abi3-py310` wheels supporting Python 3.10 through 3.14.

**Goal**: drop-in replacement that passes CPython's own `test_pathlib.py` with 2-4x less memory and 3-10x faster operations.

## Architecture

```
Python callers (from pathlibrs import Path)
        │ PyO3 boundary
┌───────┴──────────────────────────────────┐
│  PyO3 #[pyclass] layer (pure.rs,         │
│  concrete.rs) — thin wrappers            │
│  PurePath, PurePosixPath, PureWindowsPath│
│  Path, PosixPath, WindowsPath            │
└───────┬──────────────────────────────────┘
        │
┌───────┴──────────────────────────────────┐
│  Rust core (no PyO3 deps)                │
│  repr.rs    — PathRepr, ParsedPath       │
│  parsing.rs — drive/root/parts parsing   │
│  ops.rs     — stem, suffix, parent, etc. │
│  pattern.rs — fnmatch / glob patterns    │
│  iter.rs    — parts/parents iterators    │
│  fs.rs      — stat, exists, PathInfo     │
└──────────────────────────────────────────┘
```

Key design choices:
- **Lazy parsing**: `PathRepr` stores an `OsString` + `OnceCell<Box<ParsedPath>>`. Parsed on first access.
- **Separate Rust core**: All logic in testable, PyO3-free Rust modules. PyO3 classes are thin wrappers.
- **No `_flavour` object**: Platform dispatch via compile-time `cfg` + traits — zero runtime overhead.
- **GIL release during I/O**: All `stat`, `mkdir`, `unlink` calls release the GIL before syscalls.

## File Map

| Source | Purpose |
|--------|---------|
| `src/lib.rs` | PyO3 module init, re-exports, `from_os_bytes` helper |
| `src/repr.rs` | `PathRepr` struct, `ParsedPath`, lazy parsing |
| `src/parsing.rs` | POSIX and Windows path parsers |
| `src/ops.rs` | Pure path operations (stem, suffix, parent, etc.) |
| `src/pattern.rs` | Glob/fnmatch pattern compilation and matching |
| `src/iter.rs` | Iterator types (`PartsIter`, `ParentsIter`) |
| `src/pure.rs` | `PurePath`, `PurePosixPath`, `PureWindowsPath` PyO3 classes |
| `src/concrete.rs` | `Path`, `PosixPath`, `WindowsPath` PyO3 classes |
| `src/fs.rs` | Filesystem operations: `stat`, `exists`, `is_dir`, `PathInfo`, `expanduser`, `resolve` |

## Build System

This project uses three build tools:

1. **Cargo** — Rust compilation, unit tests, formatting, clippy
2. **Maturin** — builds Python wheels from the Rust crate (`pyproject.toml`)
3. **uv** — Python dependency management (dev deps, pytest, ruff)

All day-to-day commands are wrapped behind `make` targets. Use `make` for everything;
the Makefile is the single source of truth for how CI invokes any command.

### Prerequisites

- Rust toolchain (stable) with `clippy` and `rustfmt` components
- Python 3.10+ with `uv` installed

```bash
# Install uv if needed
curl -LsSf https://astral.sh/uv/install.sh | sh
```

### First-Time Setup

```bash
make install     # uv sync + maturin develop
```

## Makefile Targets

All development commands are wrapped behind `make`. Run `make` or `make help` to see
the current target listing — the Makefile is self-documenting.

### Setup & Install

| Target | Description |
|--------|-------------|
| `make setup` | Install Python dev dependencies (`uv sync --group dev`) |
| `make install` | Setup + build and install pathlibrs in dev mode (`maturin develop`) |
| `make dev` | Alias for `install` |

### Build

| Target | Description |
|--------|-------------|
| `make build` | Debug build (Rust only, no Python module) |
| `make build-release` | Release build with LTO |
| `make wheel` | Build release wheel into `dist/` |

### Test

| Target | Description |
|--------|-------------|
| `make test` | All tests (Rust + Python) |
| `make test-rust` | Rust unit tests only (`cargo test`) |
| `make test-python` | Python test suite (`pytest tests/ -v`) |

### Format

| Target | Description |
|--------|-------------|
| `make fmt` | Format everything (Rust + Python, modifies files) |
| `make fmt-rust` | Format Rust code (`cargo fmt`) |
| `make fmt-python` | Format Python code (`ruff format .`) |
| `make fmt-check` | Check formatting without modifying (CI) |
| `make fmt-check-rust` | Check Rust formatting (`cargo fmt --check --verbose`) |
| `make fmt-check-python` | Check Python formatting (`ruff format --check .`) |

### Lint

| Target | Description |
|--------|-------------|
| `make lint` | Lint everything (Rust + Python) |
| `make lint-rust` | Rust clippy with `-D warnings` |
| `make lint-python` | Python ruff check |

### CI & Cleanup

| Target | Description |
|--------|-------------|
| `make check` | Format check + lint + tests — what to run before committing |
| `make ci` | Full CI pipeline: format check, clippy, rust tests, setup, python tests |
| `make clean` | Remove build artifacts (`cargo clean` + dist/build/cache dirs) |

CI uses the same `make` targets as local development — there is no drift.

### Running Individual Commands

When you need to pass extra flags, drop down to the underlying tool:

```bash
cargo test -p pathlibrs -- --nocapture         # Rust test with stdout
uv run pytest tests/ -k "test_join" -x         # single Python test, stop on first failure
uv run maturin build --release --out dist/     # build wheel to specific dir
```

## Testing Strategy

### Rust Unit Tests

Fast, pure-Rust tests in `src/` modules. Cover parsing, operations, pattern matching. Run with `cargo test`.

### Python Smoke Tests

`tests/test_basic.py` — basic functionality tests. 65 tests covering the public API.

### Vendored CPython Test Suite

`tests/vendored/test_pathlib.py` is an **unmodified** snapshot of CPython 3.14.6's test suite. It is the acceptance criteria: a passing test = correct behavior.

`tests/skips.txt` lists tests to skip because they access CPython private API (`_flavour`, `_NormalAccessor`, or any `_`-prefixed internals). **Only private-API tests should be skipped.** A public-API test in `skips.txt` is a bug to fix.

The test runner (`tests/conftest.py`) handles import redirection and skip logic. Do not modify vendored test files — add skips to `skips.txt` instead.

## Code Style

### Rust

- Edition 2021. Standard rustfmt (default config). Clean clippy with `-D warnings`.
- Small focused modules. Each source file does one thing.
- Public API through PyO3 uses `#[pymethods]` on `#[pyclass]` structs.
- Internal Rust core uses plain functions, no PyO3 dependencies.
- No unsafe except the `from_os_bytes` helper in `lib.rs` (documented, minimal).

### Python

- `ruff` with `line-length = 100`. Rules: E, W, F, I, N, UP, B, SIM.
- Docstrings: NumPy style.
- Type hints on all functions including tests (`def test_foo() -> None:`).

## Conventions

- **CLAUDE.md** is a symlink to this file. Both paths work equivalently.
- **Don't modify vendored test files.** They are snapshots of CPython source. Changes go in `skips.txt` or `conftest.py`.
- **Error messages match CPython wording** where the test suite checks for it. Use `thiserror` for custom errors with `From<PathError> for PyErr` at the boundary.
- **New methods**: implement in the Rust core first (e.g., `ops.rs` or `fs.rs`), then expose through a thin `#[pymethod]` on the PyO3 class.
- **Commits**: gitmoji conventional commits (`✨`, `🐛`, `♻️`, etc.). See recent git log for style.
- **PRs**: conventional commit titles, summary/context/changes/test-plan body format.

## CI/CD

CI runs on every push to `main` and every PR. The workflow lives at `.github/workflows/ci.yml`.

### Test Matrix

The `test` job runs across a full matrix:

- **OS**: ubuntu-latest, macos-latest, windows-latest
- **Python**: 3.10 (minimum) and 3.14 (latest) — the abi3 wheel covers everything between

Each job runs the same `make` targets you run locally:

```
make fmt-check-rust   → cargo fmt --check --verbose
make lint-rust        → cargo clippy --all-targets -- -D warnings
make test-rust        → cargo test
make setup            → uv sync --group dev
make test-python      → uv run pytest tests/ -v
```

### Build Job

The `build` job produces abi3-py310 wheels on all three platforms:

```
make wheel            → uv run maturin build --release --out dist
```

Wheels are uploaded as artifacts. A single wheel works on Python 3.10 through 3.14.

### Local CI Check

Run the full pipeline before pushing:

```bash
make ci
```

This is identical to what CI does — no drift between local and remote verification.

## Implementation Phases

| Phase | Description | Status |
|-------|-------------|--------|
| Phase 1 | Pure Paths (no I/O) | Complete |
| Phase 2 | Filesystem Properties (stat, exists, is_dir, etc.) | Complete |
| Phase 3 | Filesystem Mutations & I/O (mkdir, unlink, read/write, copy, move, delete) | Next |
| Phase 4 | Glob & Pattern Matching (glob, rglob) | Upcoming |
| Phase 5 | Parity & Maintenance (benchmarks, skips.txt audit, upstream tracking) | Upcoming |

Full design doc: `DESIGN.md`. Refer to it for architecture decisions, error handling strategy, and resolved design questions.
