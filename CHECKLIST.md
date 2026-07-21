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

## Phase 3: Filesystem Mutations & I/O — Complete

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

- [x] `open()` — delegate to Python `io.open()`
- [x] `read_bytes()`, `read_text()`
- [x] `write_bytes()`, `write_text()`

### Directory Traversal

- [x] `iterdir()`
- [x] `walk()` with `topdown`, `bottomup`, `onerror`, `follow_symlinks`

### 3.14 File-Tree Operations

- [x] `copy()` — copy file or directory tree to exact target
- [x] `copy_into()` — copy into an existing directory
- [x] `move()` — move file or directory tree to exact target
- [x] `move_into()` — move into an existing directory
- [x] `delete()` — recursively delete file or directory tree
- [x] `_delete()` — CPython private-API alias for `delete()`

### Verification

- [x] All vendored CPython tests pass
- [x] CI passes on all platforms: Linux, macOS, Windows (Python 3.10 + 3.14)

## Phase 4: Glob & Pattern Matching — Complete

- [x] `glob()` with full pattern syntax: `**`, `*`, `?`, `[abc]`, `[!abc]`
- [x] `rglob()` with full pattern syntax
- [x] Brace expansion in patterns
- [x] `case_sensitive` kwarg (3.12+)
- [x] `recurse_symlinks` kwarg (3.13+)
- [x] Symlink loop detection for recursive globs
- [x] Glob iterator bridging (Rust iterator → Python iterator protocol)
- [x] All vendored CPython glob tests pass

## Phase 5: Parity & Maintenance — Closing

Vendored CPython 3.14.6 test suite: **810 passed, 394 skipped, 0 failures**.
**2 active skip entries** (down from 239 baseline — 237 resolved).

### Feature Parity — Complete

- [x] `Path.home()`, `Path.cwd()` class methods
- [x] Pure path edge cases: name/stem/parts for empty/`.` paths
- [x] `__repr__` uses dynamic class name
- [x] `__bytes__` and bytes type validation
- [x] `with_name()`/`with_stem()` reject empty/reserved names
- [x] `as_uri()` percent-encoding via `urllib.parse.quote`
- [x] `__eq__` matches CPython 3.14: returns NotImplemented for non-PurePath types
- [x] Cross-flavour equality: `PurePosixPath('a') != PureWindowsPath('a')`
- [x] Cross-flavour ordering: `PurePosixPath('a') < PureWindowsPath('a')` raises TypeError
- [x] `is_reserved()` method with DeprecationWarning
- [x] Path/PosixPath constructors accept `os.PathLike` objects
- [x] Path multi-arg constructor normalizes separators
- [x] `relative_to()` rejects `..` segments
- [x] Subclass pickle/protocol support
- [x] Constructor rejects unknown kwargs with TypeError
- [x] PurePosixPath(PureWindowsPath(...)) cross-flavour construction
- [x] `from_uri()` Windows support (DOS drive letters, UNC, pipe notation)
- [x] `from_uri()` POSIX support
- [x] `owner()`/`group()` raise UnsupportedOperation on Windows-flavoured paths
- [x] `resolve()` cross-platform: canonicalize on POSIX, read_link on Windows
- [x] Windows symlink+`..` lexical cancellation
- [x] `absolute()` drive-relative path CWD on Windows
- [x] Windows UNC/device/extended-path edge cases — 37 Rust unit tests covering all forms

### Remaining Skips — 2 entries (both permanently unfixable)

| Skip | Blocker |
|------|---------|
| `PurePathTest.test_concrete_class` | PyO3 `#[new]` must return `Self` — cannot auto-dispatch `PurePath('a')` to `PurePosixPath` |
| `PathTest.test_delete_unwritable` | Windows `FILE_ATTRIBUTE_READONLY` on directories doesn't prevent file deletion inside |

### Infrastructure — Complete

- [x] Performance benchmark suite (`benchmarks/`) — 84 tests, 7 categories
- [x] Release-mode benchmarks (`make bench` builds via `maturin develop --release`)
- [x] CI benchmark workflow with baseline storage and PR regression comparison
- [x] Published benchmark results (`BENCHMARKS.md`, CI step summary)
- [x] Automated upstream test sync workflow (`scripts/sync_vendored_tests.py` + `make sync-vendored` + weekly CI)

## CI / Infrastructure

- [x] AGENTS.md with project overview and agent instructions
- [x] CLAUDE.md symlinked to AGENTS.md
- [x] Makefile with self-documenting `make help`
- [x] `.pre-commit-config.yaml` with Rust + Python hooks
- [x] CI workflow (`.github/workflows/ci.yml`) — full Python matrix (3.10-3.14) on Linux/macOS/Windows
- [x] CI benchmark job with baseline artifact storage and PR regression comment
- [x] Vendored CPython 3.14.6 test suite
- [x] `tests/conftest.py` with `--windows-flavour` support
- [x] `pathlib._local` shim for CPython 3.13 unpickling
- [x] `isjunction` shim for Python < 3.12
- [x] `pathname2url(add_scheme=True)` shim for Python < 3.14
- [x] `infinite_recursion` monkey-patch for Python < 3.11
- [x] `subst_drive` shim for Python < 3.14
- [ ] Automated upstream test sync workflow
