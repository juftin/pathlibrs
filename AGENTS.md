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

| Source            | Purpose                                                                                |
| ----------------- | -------------------------------------------------------------------------------------- |
| `src/lib.rs`      | PyO3 module init, re-exports, `from_os_bytes` helper                                   |
| `src/repr.rs`     | `PathRepr` struct, `ParsedPath`, lazy parsing                                          |
| `src/parsing.rs`  | POSIX and Windows path parsers                                                         |
| `src/ops.rs`      | Pure path operations (stem, suffix, parent, etc.)                                      |
| `src/pattern.rs`  | Glob/fnmatch pattern compilation and matching                                          |
| `src/iter.rs`     | Iterator types (`PartsIter`, `ParentsIter`)                                            |
| `src/pure.rs`     | `PurePath`, `PurePosixPath`, `PureWindowsPath` PyO3 classes                            |
| `src/concrete.rs` | `Path`, `PosixPath`, `WindowsPath` PyO3 classes                                        |
| `src/fs.rs`       | Filesystem operations: `stat`, `exists`, `is_dir`, `PathInfo`, `expanduser`, `resolve` |

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

| Target         | Description                                                         |
| -------------- | ------------------------------------------------------------------- |
| `make setup`   | Install Python dev dependencies (`uv sync --group dev`)             |
| `make install` | Setup + build and install pathlibrs in dev mode (`maturin develop`) |
| `make dev`     | Alias for `install`                                                 |

### Build

| Target               | Description                               |
| -------------------- | ----------------------------------------- |
| `make build`         | Debug build (Rust only, no Python module) |
| `make build-release` | Release build with LTO                    |
| `make wheel`         | Build release wheel into `dist/`          |

### Test

| Target              | Description                                          |
| ------------------- | ---------------------------------------------------- |
| `make test`         | All tests (Rust + Python)                            |
| `make test-rust`    | Rust unit tests only (`cargo test`)                  |
| `make test-python`  | Python test suite (`pytest tests/ -v`)               |
| `make test-windows` | Run Windows-flavour tests on any host OS (see below) |

### Format

| Target                  | Description                                           |
| ----------------------- | ----------------------------------------------------- |
| `make fmt`              | Format everything (Rust + Python, modifies files)     |
| `make fmt-rust`         | Format Rust code (`cargo fmt`)                        |
| `make fmt-python`       | Format Python code (`ruff format .`)                  |
| `make fmt-check`        | Check formatting without modifying (CI)               |
| `make fmt-check-rust`   | Check Rust formatting (`cargo fmt --check --verbose`) |
| `make fmt-check-python` | Check Python formatting (`ruff format --check .`)     |

### Lint

| Target             | Description                     |
| ------------------ | ------------------------------- |
| `make lint`        | Lint everything (Rust + Python) |
| `make lint-rust`   | Rust clippy with `-D warnings`  |
| `make lint-python` | Python ruff check               |

### CI & Cleanup

| Target               | Description                                                             |
| -------------------- | ----------------------------------------------------------------------- |
| `make check`         | Format check + lint + tests — what to run before committing             |
| `make ci`            | Full CI pipeline: format check, clippy, rust tests, setup, python tests |
| `make hooks`         | Run all pre-commit hooks on all files                                   |
| `make hooks-install` | Install pre-commit hooks into `.git/hooks`                              |
| `make clean`         | Remove build artifacts (`cargo clean` + dist/build/cache dirs)          |

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

### Windows Testing on Linux/Mac

The vendored test suite includes `@needs_windows` tests that normally only run on
Windows CI runners. Use `--windows-flavour` to alias `PurePath → PureWindowsPath`
and validate Windows path parsing on any host OS:

```bash
make test-windows
```

**Run this locally before pushing from a Linux or Mac host.** CI tests Windows
behaviour on the Windows runner, but catching Windows-specific failures before
pushing saves a CI cycle. This exercises the full Windows path parser (drive
letters, UNC paths, extended-length prefixes) in pure Rust — no Windows OS needed.

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
- **Worktrees**: after creating a new git worktree, run `make hooks-install` inside it so pre-commit hooks fire on every commit.
- **Pre-push gate**: before pushing to GitHub, run the full local CI:
    ```bash
    make ci          # format check + clippy + rust tests + python tests
    make test-windows  # Windows-flavour tests
    ```
    Push only after both pass. This catches formatting, lint, and test failures before they hit CI.

## Troubleshooting & Common Pitfalls

### "Why are my Rust changes not reflected in test results?"

**Always use `make test-python`, never `uv run pytest` directly.** The Makefile
target depends on `install` which runs `maturin develop` to rebuild the Rust
extension. Running `uv run pytest` or `pytest` directly skips the rebuild and
tests run against the last installed build.

The `--no-sync` flag on `test-python` is **required** — removing it causes
`uv` to re-sync the venv, which blows away `maturin develop`'s editable install.

If the build cache gets stuck, use `make rebuild` to force a fresh `cargo build`
followed by `maturin develop`.

### "Tests pass locally but fail in CI on Python 3.10"

The vendored CPython 3.14 test suite uses APIs that didn't exist in Python 3.10.
`tests/conftest.py` backports these via monkey-patches:

- `assertIsSubclass` (added in Python 3.11)
- `assertStartsWith` / `assertEndsWith` (added in Python 3.12)

When adding new features that unskip vendored tests, verify they're Python
3.10-compatible. If the test uses a 3.11+ API that can't be cleanly shimmed
(e.g., `pathname2url(add_scheme=True)` whose internal behavior changed between
3.10 and 3.11+), keep the test in `skips.txt`.

### "Tests pass on macOS/Linux but fail on Windows CI"

Some tests pass on POSIX but fail on Windows due to path-parsing or symlink
differences:

- **complex_symlinks** (9 tests) — relative/dot-dot symlink resolution works
  on POSIX but not Windows. Keep in `skips.txt` until Windows support is added.
- **PathTest.test_rmdir** — Windows-specific handle leak. Keep skipped.

When removing entries from `skips.txt`, run `make test-windows` first to check
for Windows-flavour regressions. The `--windows-flavour` flag exercises the
Windows path parser on any host OS.

### "CI fails on `cargo fmt --check`"

`cargo fmt` _must_ pass before pushing. CI runs `make fmt-check-rust` first.
Always run `make ci` or at minimum `make fmt` before committing. Pre-commit
hooks (`make hooks-install`) catch this locally, but they need to be installed
in each new worktree.

### "Pre-commit hooks aren't running"

After creating a new git worktree, run `make hooks-install` to install the
hooks into `.git/hooks`. Without this, `cargo fmt`, `cargo clippy`, and other
checks won't run on commit.

### "I need to push a branch"

Push to the `jufty-bot` fork, not directly to `juftin/pathlibrs`:

```bash
git push jufty-bot resolve-skips
```

GitHub identity for all operations is `jufty-bot`. Use:

```bash
GH_CONFIG_DIR="$HOME/.config/gh-bot" gh ...
```

### "How do I check CI after pushing?"

```bash
# Wait ~90 seconds after push, then:
GH_CONFIG_DIR="$HOME/.config/gh-bot" gh run list \
  --repo juftin/pathlibrs --branch resolve-skips --limit 1

# Watch until completion:
GH_CONFIG_DIR="$HOME/.config/gh-bot" gh run watch <run-id> \
  --repo juftin/pathlibrs --exit-status
```

## Pre-Commit Hooks

Pre-commit runs on every `git commit` to catch issues before they hit CI.

```bash
make hooks-install   # one-time: install hooks into .git/hooks
make hooks           # run all hooks on all files (useful for CI or bulk fixups)
```

Hooks configured in `.pre-commit-config.yaml`:

| Hook                                                | What it does                                        |
| --------------------------------------------------- | --------------------------------------------------- |
| `trailing-whitespace`                               | Strips trailing whitespace                          |
| `end-of-file-fixer`                                 | Ensures files end with a newline                    |
| `check-yaml` / `check-ast` / `check-merge-conflict` | Syntax and conflict checks                          |
| `mixed-line-ending`                                 | Enforces consistent line endings                    |
| `no-commit-to-branch`                               | Blocks commits directly to `main`                   |
| `pretty-format-toml`                                | Formats TOML files (excludes `uv.lock`)             |
| `cargo-fmt`                                         | Formats Rust code (`cargo fmt`)                     |
| `cargo-clippy`                                      | Lints Rust code (`cargo clippy -- -D warnings`)     |
| `ruff-format`                                       | Formats Python code                                 |
| `ruff-check --fix`                                  | Lints and auto-fixes Python code                    |
| `prettier`                                          | Formats YAML, JSON, Markdown, etc.                  |
| `uv-lock`                                           | Regenerates `uv.lock` when `pyproject.toml` changes |

**Vendored tests** (`tests/vendored/`) are excluded from all modifying hooks.
They are unmodified CPython snapshots — never format or lint them.

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

| Phase   | Description                                                                | Status   |
| ------- | -------------------------------------------------------------------------- | -------- |
| Phase 1 | Pure Paths (no I/O)                                                        | Complete |
| Phase 2 | Filesystem Properties (stat, exists, is_dir, etc.)                         | Complete |
| Phase 3 | Filesystem Mutations & I/O (mkdir, unlink, read/write, copy, move, delete) | Complete |
| Phase 4 | Glob & Pattern Matching (glob, rglob)                                      | Complete |
| Phase 5 | Parity & Maintenance (benchmarks, skips.txt audit, upstream tracking)      | Closing  |

Full design doc: `DESIGN.md`. Refer to it for architecture decisions, error handling strategy, and resolved design questions.

## Working with the Checklist

`CHECKLIST.md` is the authoritative task tracker for this project. Agents must:

- **Before starting work** — read `CHECKLIST.md` to understand what phase is active,
  what's already done, and what's next.
- **When completing an item** — check it off in `CHECKLIST.md` by changing
  `- [ ]` to `- [x]`. Update the skip count in the Phase 3 header if new
  vendored tests are passing.
- **When discovering new work** — add it to `CHECKLIST.md` under the appropriate
  phase rather than keeping it in conversation context.
- **Before claiming a phase is complete** — verify every unchecked item in that
  phase is addressed and the skip count is zero for public API tests.
