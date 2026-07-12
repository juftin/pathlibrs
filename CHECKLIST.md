# Development Checklist

## Phase 1: Pure Paths — Complete

- [x] `PathRepr` struct with lazy parsing
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
- [x] Python subclassing support via `#[pyclass(subclass)]`
- [x] 36 Rust unit tests
- [x] 65 Python smoke tests
- [x] All vendored pure-path CPython tests pass

## Phase 2: Filesystem Properties — Complete

- [x] `stat()`, `lstat()` — returns `StatResult` with all metadata fields
- [x] `exists()`, `is_dir()`, `is_file()`, `is_symlink()`
- [x] `is_mount()`, `is_junction()`
- [x] `PathInfo` — cached stat result (3.12+)
- [x] `samefile()`
- [x] `owner()`, `group()`
- [x] `resolve()`, `absolute()`
- [x] `readlink()`
- [x] `expanduser()` (POSIX and Windows)
- [x] GIL release during all I/O syscalls
- [x] Path classes: `Path`, `PosixPath`, `WindowsPath` (concrete)

## Phase 3: Filesystem Mutations & I/O — Substantially Complete ✓

902 test skips in `skips.txt` (down from 1030; 127 new test passes from ~130 unskipped vendored CPython tests).

531 active skip entries (down from 656; 126 removed from `skips.txt`).

### Directory Mutations

- [x] `mkdir()` with `mode`, `parents`, `exist_ok`
- [x] `rmdir()`
- [x] `chmod()`, `lchmod()`

### File Mutations

- [x] `touch()` with `mode`, `exist_ok`
- [x] `unlink()` with `missing_ok`
- [x] `rename()`, `replace()`
- [x] `symlink_to()`, `hardlink_to()`

### I/O

- [x] `open()` — delegate to Python `io.open()` per DESIGN.md §11.1
- [x] `read_bytes()`, `read_text()`
- [x] `write_bytes()`, `write_text()`

### Directory Traversal

- [x] `iterdir()`
- [x] `walk()` with `topdown`, `bottomup`, `onerror`, `follow_symlinks` (basic; complex edge cases TBD)

### 3.14 File-Tree Operations

- [x] `copy()` — copy file or directory tree to exact target (matching CPython semantics)
- [x] `copy_into()` — copy into an existing directory
- [x] `move()` — move file or directory tree to exact target (matching CPython semantics)
- [x] `move_into()` — move into an existing directory
- [x] `delete()` — delete file or directory tree (basic; private-API `_delete()` tests skipped)

### Verification

- [x] All basic mutation and I/O vendored CPython tests pass (63→127 new passes)
- [ ] 3.14 file-tree operation edge case tests pass (complex cases remain skipped)
- [x] `copy()` and `move()` match CPython semantics: exact-target copy with `ensure_distinct_paths` guards
- [x] GIL released during all blocking I/O
- [x] CI passes on all platforms: Linux, macOS, Windows (Python 3.10 + 3.14)

## Phase 4: Glob & Pattern Matching — Complete

- [x] `glob()` with full pattern syntax: `**`, `*`, `?`, `[abc]`, `[!abc]`
- [x] `rglob()` with full pattern syntax
- [x] Brace expansion in patterns
- [x] `case_sensitive` kwarg (3.12+)
- [x] `recurse_symlinks` kwarg (3.13+)
- [x] Symlink loop detection for recursive globs
- [x] Glob iterator bridging (Rust iterator → Python iterator protocol)
- [x] `glob.rs` module extracted from `iter.rs` / `pattern.rs`
- [x] Verify: all vendored CPython glob tests pass across platform matrix (51/51 non-Windows tests, Windows tests run on Windows CI)

## Phase 5: Parity & Maintenance — In Progress

594 passed, 610 skipped (up from 552 passed, 652 skipped).
239 active skip entries (down from 305).

### Feature Parity

- [x] `Path.home()`, `Path.cwd()` class methods — verified passing for PathSubclassTest
- [x] Pure path edge cases: name/stem/parts for empty/`.` paths — fixed
- [x] `__repr__` uses dynamic class name — fixed
- [x] `__bytes__` and bytes type validation — fixed
- [x] `with_name()`/`with_stem()` reject empty/reserved names — fixed
- [x] `as_uri()` percent-encoding via `urllib.parse.quote` — fixed (absolute check + encode)
- [x] `__eq__` matches CPython 3.14: returns NotImplemented for non-PurePath types
- [x] Added `preserve_metadata` kwarg to `copy()` and `copy_into()` (no-op; signature accepted)
- [x] Fixed symlink copy: no longer overwrites existing target with `follow_symlinks=False`
- [x] Fixed `full_match`: `*` in non-last segments now uses fnmatch (was exact-match-only)
- [x] Fixed `match()`: raises ValueError for empty/`.` patterns; empty path returns False
- [x] Fixed `move_tree()`: only falls back to copy+delete on EXDEV (cross-device), not all errors
- [ ] Windows UNC/device/extended-path edge cases (DESIGN.md §4.8)
- [ ] Symlink edge cases on Linux/macOS
- [ ] Full pickle / `__reduce__` / `__fspath__` / `copy` coverage

### Skip Audit

- [x] Batch 1: PathSubclassTest-only entries audited and removed (35 entries)
- [x] Batch 2: Pure path edge cases fixed and unskipped (41 entries)
- [x] Batch 3: repr + bytes handling fixed and unskipped (30 entries)
- [x] Batch 4: as_uri() fixed — 20 tests unskipped, 22 entries removed from skips.txt
- [x] Batch 5: Equality + parse audit — 25 tests unskipped, 25 entries removed
- [x] Batch 6: Delete audit — all 26 entries are private `_delete()` API, kept skipped
- [x] Batch 7: Copy audit — 21 entries unskipped (9 self-copy + 12 existing-symlink), bugs fixed
- [x] Batch 8: Match audit — 27 entries unskipped (match_common, match_empty, full_match_case_sensitive), bugs fixed
- [x] Batch 9: Move audit — 43 entries unskipped, `move_tree` fixed to only fall back on EXDEV
- [ ] Remaining audit: 239 entries across windows(30), parse_windows(6), relative_to(7), equivalences(13), symlinks(14), resolve(12), copy(12 remaining), delete(26 kept), move remaining(2)
- [ ] Classify each skip as private API, fixable, or platform-specific
- [ ] Goal: `skips.txt` contains _only_ private-API entries
- [ ] Goal: zero public-API `NotImplemented` entries

### Automated Vendored Test Tracking

- [ ] CI workflow to periodically fetch latest CPython `test_pathlib.py`
- [ ] Auto-open issue/PR on upstream test changes
- [ ] Run updated test suite against `pathlibrs` automatically

### Performance Benchmarks

- [ ] Pure operations: `.parent`, `.stem`, `.suffix`, `.name`, `.with_name()`, `/`, `__str__`
- [ ] Stat: `.exists()`, `.is_file()`, `.is_dir()`, `.stat()` (hot + cold cache)
- [ ] I/O: `.read_text()`, `.write_text()`, `.read_bytes()`, `.write_bytes()`
- [ ] Directory: `.iterdir()`, `.walk()` on varied tree shapes
- [ ] Glob: `.glob()`, `.rglob()` on small, medium, and deep trees
- [ ] Mutations: `.mkdir()`, `.unlink()`, `.rename()`, `.symlink_to()`, `.copy()`, `.move()`, `.delete()`
- [ ] Memory: object size (100k instances), allocation count, peak RSS during `rglob`
- [ ] CI workflow runs benchmarks on every push to main
- [ ] Results published in `docs/benchmarks.md` + JSON archive
- [ ] Regression alerting if any benchmark regresses >10%

### Acceptance Criteria

- [ ] Full vendored CPython 3.14 test suite passes on all platforms (3.10, 3.14)
- [ ] `skips.txt` contains only private-API entries
- [ ] Automated upstream test tracking in place and passing CI
- [ ] Benchmark suite runs in CI with publishable results
- [ ] Performance ≥ parity with built-in `pathlib` on all metrics

## CI / Infrastructure

- [x] AGENTS.md with project overview and agent instructions
- [x] CLAUDE.md symlinked to AGENTS.md
- [x] Makefile with self-documenting `make help`
- [x] `.pre-commit-config.yaml` with Rust + Python hooks
- [x] CI workflow (`.github/workflows/ci.yml`) using Make targets
- [x] Vendored CPython 3.14.6 test suite
- [x] `tests/conftest.py` with `--windows-flavour` support
- [ ] Automated upstream test sync workflow (`.github/workflows/vendored-sync.yml`)
- [ ] Automated benchmark workflow (`.github/workflows/benchmarks.yml`)
- [ ] Benchmark fixtures and helpers (`benchmarks/`)
- [ ] Published benchmark results (`docs/benchmarks.md`)
