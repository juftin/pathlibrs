# Development Checklist

## Phase 1: Pure Paths — Complete

- [x] `PathRepr` with lazy parsing
- [x] `PurePath`, `PurePosixPath`, `PureWindowsPath` PyO3 classes
- [x] Properties: `parts`, `drive`, `root`, `anchor`, `parent`, `parents`, `name`, `suffix`, `suffixes`, `stem`
- [x] `joinpath()`, `with_name()`, `with_stem()`, `with_suffix()`, `with_segments()`
- [x] `relative_to()` with `walk_up` kwarg (3.12+)
- [x] `is_relative_to()`
- [x] `as_posix()`, `as_uri()`, `from_uri()`
- [x] `match()` and `full_match()` with `case_sensitive` kwarg (3.13+)
- [x] Dunders: `__str__`, `__repr__`, `__fspath__`, `__eq__`, `__hash__`, `__lt__`
- [x] `/` operator (`__truediv__`, `__rtruediv__`)
- [x] Pickle / `__reduce__` support
- [x] POSIX and Windows parsing in pure Rust
- [x] Glob pattern matching (fnmatch-style)
- [x] Vendored CPython 3.14.6 test suite runner
- [x] `parser` class attribute (posixpath / ntpath)
- [x] Smoke test suite (65 tests)
- [x] CPython pure-path tests pass
- [x] All path classes support Python subclassing via `#[pyclass(subclass)]`
- [x] Rust unit tests (36 tests)

## Phase 2: Filesystem Properties — Complete

- [x] `stat()`, `lstat()`
- [x] `exists()`, `is_dir()`, `is_file()`, `is_symlink()`, `is_mount()`, `is_junction()`
- [x] `PathInfo` — cached stat result (3.12+)
- [x] `samefile()`
- [x] `owner()`, `group()`
- [x] `resolve()`, `absolute()`, `readlink()`
- [x] `expanduser()`

## Phase 3: Filesystem Mutations & I/O — Next

- [ ] `mkdir()` (with `mode`, `parents`, `exist_ok`)
- [ ] `rmdir()`
- [ ] `unlink()` (with `missing_ok`)
- [ ] `rename()`, `replace()`
- [ ] `symlink_to()`, `hardlink_to()`
- [ ] `touch()` (with `mode`, `exist_ok`)
- [ ] `chmod()`, `lchmod()`
- [ ] `open()` (delegate to Python `io.open()`)
- [ ] `read_bytes()`, `read_text()`
- [ ] `write_bytes()`, `write_text()`
- [ ] `iterdir()`
- [ ] `walk()` (topdown/bottomup, onerror, follow_symlinks)
- [ ] `copy()`, `copy_into()` (3.14)
- [ ] `move()`, `move_into()` (3.14)
- [ ] `delete()` (3.14)
- [ ] Verify: all mutation, I/O, and 3.14 file-tree tests pass

## Phase 4: Glob & Pattern Matching — Upcoming

- [ ] `glob()` with full pattern syntax: `**`, `*`, `?`, `[abc]`, `[!abc]`
- [ ] `rglob()` with full pattern syntax
- [ ] `case_sensitive` kwarg (3.12+)
- [ ] `recurse_symlinks` kwarg (3.13+)
- [ ] Symlink loop detection for recursive globs
- [ ] Glob iterator bridging (Rust → Python iterator protocol)
- [ ] `glob.rs` module extracted from `iter.rs` / `pattern.rs`
- [ ] Verify: all vendored CPython glob tests pass across platform matrix

## Phase 5: Parity & Maintenance — Upcoming

- [ ] `Path.home()`, `Path.cwd()` class methods
- [ ] Windows UNC/device/extended-path edge cases
- [ ] Symlink edge cases on Linux/macOS
- [ ] Pickle / `__reduce__` / `__fspath__` / `copy` full coverage
- [ ] Benchmark suite against CPython pathlib
- [ ] **Skip audit** — drive `skips.txt` to private-API only
  - [ ] Audit every entry in `tests/skips.txt`
  - [ ] Each skip is either private API or implemented
  - [ ] Goal: zero public-API entries in `skips.txt`
- [ ] **Automated vendored test tracking**
  - [ ] CI workflow to fetch latest CPython `test_pathlib.py`
  - [ ] Auto-open issue/PR on upstream test changes
- [ ] **Performance testing & benchmarking**
  - [ ] Pure operations benchmark
  - [ ] Stat operations benchmark
  - [ ] I/O operations benchmark
  - [ ] Directory traversal benchmark
  - [ ] Glob benchmark (Phase 4)
  - [ ] Mutations benchmark
  - [ ] Memory benchmark
  - [ ] CI workflow with regression alerting (>10% regression)
- [ ] Acceptance: full CPython 3.14 test suite passes on all platforms (3.10–3.14)
- [ ] Acceptance: `skips.txt` contains only private-API entries
- [ ] Acceptance: benchmark suite runs in CI

## CI / Infrastructure

- [ ] Automated upstream test sync workflow (`.github/workflows/vendored-sync.yml`)
- [ ] Automated benchmark workflow (`.github/workflows/benchmarks.yml`)
- [ ] Benchmark fixtures and helpers (`benchmarks/`)
- [ ] Published benchmark results (`docs/benchmarks.md`)
