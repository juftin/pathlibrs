# Design Doc: `pathlibrs` — A Rust Rewrite of Python's `pathlib` via PyO3

## 1. Motivation

Python's `pathlib` (`Lib/pathlib.py`) is one of the most commonly imported standard library modules. Every filesystem read, write, or traversal passes through it. Yet its implementation is pure Python with three fundamental performance problems:

1. **Memory bloat** — Each `Path` object carries a full Python `str` (49+ bytes + object overhead) plus a `_flavour` object and cached properties. A `PosixPath("/usr/local/bin")` weighs ~160+ bytes in CPython. An equivalent Rust `PathBuf` is 24 bytes.
2. **String allocation churn** — Operations like `.parent`, `.stem`, `.with_suffix()`, and `.joinpath()` allocate new Python `str` objects on every call, which then get garbage collected.
3. **Serial method dispatch** — All method resolution goes through Python's MRO, attribute lookup, and `_flavour` routing. Rust can monomorphize or use static dispatch.

Goal: a drop-in replacement that passes CPython's own `test_pathlib.py` while using 2–4× less memory and completing common operations 3–10× faster.

---

## 2. Architecture Overview

```
┌─────────────────────────────────────────────────┐
│  Python Callers                                  │
│  from pathlibrs import Path                      │
└─────────────────────┬───────────────────────────┘
                      │ PyO3 boundary
┌─────────────────────┴───────────────────────────┐
│  pathlibrs (Rust crate)                          │
│                                                   │
│  ┌─────────────┐  ┌──────────────┐               │
│  │  PyO3 module │  │  Maturin     │               │
│  │  init+types  │  │  build sys   │               │
│  └──────┬──────┘  └──────────────┘               │
│         │                                          │
│  ┌──────┴─────────────────────────┐               │
│  │  Class Layer (PyO3 #[pyclass]) │               │
│  │  - PurePath, PurePosixPath     │               │
│  │  - PureWindowsPath, Path       │               │
│  │  - PosixPath, WindowsPath      │               │
│  └──────┬─────────────────────────┘               │
│         │                                          │
│  ┌──────┴─────────────────────────┐               │
│  │  Rust Core (no PyO3 deps)      │               │
│  │  - path_buf: PathBuf/OsString  │               │
│  │  - parsing: drive, root, parts │               │
│  │  - ops: stem, suffix, parent   │               │
│  │  - glob: pattern, rglob        │               │
│  │  - fs: stat, exists, unlink    │               │
│  └─────────────────────────────────┘               │
└─────────────────────────────────────────────────────┘
```

The critical design choice: **separate the Rust core from the PyO3 boundary**. The core does all the work in safe, testable Rust. The PyO3 layer is a thin wrapper that translates Python method calls into Rust trait methods.

---

## 3. Class Hierarchy

### 3.1 Python View (preserved exactly)

```
PurePath
├── PurePosixPath
└── PureWindowsPath

Path (inherits from PurePath)
├── PosixPath
└── WindowsPath
```

### 3.2 Rust Internal Representation

```rust
// Internal representation — minimal, no Python overhead
struct PathRepr {
    buf: OsString,
    // Parsed fields cached on first access.
    // Boxed so the inline size of PathRepr stays small (~32 bytes)
    // regardless of how large ParsedPath grows.
    parsed: OnceCell<Box<ParsedPath>>,
}

struct ParsedPath {
    drive: Option<OsString>,       // "C:" or None
    root: Option<OsString>,        // "\\" or "/" or None
    parts: Vec<OsString>,          // parsed components
    anchor_length: usize,          // len(drive) + len(root)
}
```

### 3.3 PyO3 Type Hierarchy

```rust
#[pyclass(subclass)]
struct PurePath {
    inner: PathRepr,
}

#[pyclass(extends=PurePath)]
struct PurePosixPath { /* marker, no extra data */ }

#[pyclass(extends=PurePath)]
struct PureWindowsPath {
    // On non-Windows hosts, PureWindowsPath still uses
    // a Windows-aware parser with drive/UNC support
    inferred_drive: OnceCell<Option<String>>,
}

#[pyclass(extends=PurePath)]
struct Path {
    // No extra data — all IO ops dispatch to std::fs
}

// PosixPath and WindowsPath are subclasses of Path
// that add platform-specific behaviour and override
// _flavour-like dispatch at the Rust level.
```

**Key decision — no `_flavour` object.** In CPython, `_flavour` carries string operations (case sensitivity, path separators). In Rust, these are compile-time constants or match arms on an enum — zero overhead at runtime.

---

## 4. Core Design Decisions

### 4.1 Lazy Parsing

Don't parse the path on construction. Parse lazily when `.drive`, `.root`, `.parts`, or `.anchor` is accessed. On construction, just store the `OsString`:

```rust
impl PurePath {
    fn new(input: &OsStr) -> Self {
        Self {
            inner: PathRepr {
                buf: input.to_os_string(),
                parsed: OnceCell::new(),
            },
        }
    }

    fn parsed(&self) -> &ParsedPath {
        self.inner.parsed.get_or_init(|| Box::new(parse_path(&self.inner.buf)))
    }
}
```

This means `PurePath("/a/b/c")` allocates exactly one `OsString` (24 bytes on 64-bit) + the `OnceCell<Box<ParsedPath>>` (8 bytes — the `Box` enables niche optimization so the `Option` state is zero-cost). The `PathRepr` is **32 bytes** on stack; the full Python object via PyO3 is ~60-72 bytes including the Python object header. Compare with CPython's ~160+ bytes.

### 4.2 Zero-Copy String Operations Where Possible

Operations like `.name`, `.stem`, `.suffix` return new `OsStr` slices where possible, avoiding allocations:

```rust
fn name(&self) -> &OsStr {
    let buf = &self.inner.buf;
    let sep = path_separator_for(buf);
    if let Some(pos) = buf.rfind(sep) {
        &buf[pos + 1..]
    } else {
        buf.as_os_str()
    }
}
```

Returned as Python `str` through PyO3 (allocation at the boundary, not in the core). Compare with CPython which allocates intermediate Python strings at every step.

### 4.3 Builder Pattern for Mutations

Operations like `.with_name()`, `.with_suffix()`, `.relative_to()` return new `PathRepr` objects constructed from the parsed components. The builder pattern avoids intermediate allocations:

```rust
fn with_name(&self, name: &OsStr) -> PathRepr {
    // Single allocation: concat(parent_segment, name)
    let parent = self.parent();
    let mut new = OsString::with_capacity(parent.len() + 1 + name.len());
    new.push(parent);
    new.push(SEPARATOR);
    new.push(name);
    PathRepr::new(&new)
}
```

### 4.4 Platform Dispatch at Compile Time

CPython's `_flavour` is a runtime object with virtual methods. Rust uses conditional compilation + match on an enum:

```rust
enum PathFlavour { Posix, Windows }

fn path_separator_for(path: &OsStr) -> u8 {
    if cfg!(target_os = "windows") { b'\\' } else { b'/' }
}

// For PureWindowsPath on Linux:
fn path_separator_windows(_: &OsStr) -> u8 { b'\\' }
fn path_separator_posix(_: &OsStr) -> u8 { b'/' }
```

When a `PureWindowsPath` is constructed on Linux, all its string operations use Windows path semantics (backslash separators, drive letters, UNC paths) via a trait that's resolved once at construction time.

### 4.5 Iterator Optimization

`PurePath.parts` returns an iterator that walks the path string with `split_once` — no allocation of intermediate substrings. `parents` likewise:

```rust
fn parts_iter(os_str: &OsStr) -> impl Iterator<Item = &OsStr> {
    // Enumerate parsed components from cached ParsedPath
    // or walk the OsString directly
    os_str
        .as_encoded_bytes()
        .split(|&b| b == b'/' || b == b'\\')
        .filter(|s| !s.is_empty())
        .map(|b| OsStr::from_encoded_bytes(b).unwrap())
}
```

### 4.6 Glob with WalkDir, Not Recursive listdir

CPython's `glob()` uses recursive `os.listdir` + `re` matching. Python `rglob("**/*.py")` materializes the entire tree in a list before yielding. Rust implementation uses `walkdir` to stream results:

```rust
// Rust core — returns a lazy Rust iterator
fn glob(&self, pattern: &OsStr, recursive: bool) -> impl Iterator<Item = PathBuf> {
    let compiled = GlobPattern::new(pattern);
    WalkDir::new(self.as_os_str())
        .max_depth(if recursive { usize::MAX } else { 1 })
        .into_iter()
        .filter_entry(|e| compiled.matches(e.path()))
        .filter_map(|e| e.ok())
        .map(|e| e.into_path())
}
```

The Rust iterator is wrapped in a PyO3 `#[pyclass]` that implements the Python iterator protocol (`__iter__` / `__next__`), so Python callers see a lazy generator — not a list.

**Ordering**: CPython's `glob()` uses `os.scandir()`, which returns entries in filesystem order (arbitrary, not sorted). Neither CPython nor this implementation guarantees any specific ordering. Users who need sorted results should call `sorted()` themselves. This matches CPython semantics.

### 4.7 Error Handling Strategy

PyO3 automatically maps common Rust error types to Python exceptions. Our strategy is to leverage this rather than building a parallel error system:

| Rust Error | Python Exception |
|---|---|
| `std::io::Error` | `OSError` (via PyO3 built-in conversion) |
| `std::io::ErrorKind::NotFound` | `FileNotFoundError` |
| `std::io::ErrorKind::PermissionDenied` | `PermissionError` |
| `std::io::ErrorKind::AlreadyExists` | `FileExistsError` |
| `std::io::ErrorKind::InvalidInput` | `ValueError` (for path construction) |
| `std::str::Utf8Error` | `UnicodeDecodeError` |
| `StripPrefixError` (from `relative_to`) | `ValueError` |

Custom error types in the Rust core use `thiserror` and are converted to `PyErr` at the PyO3 boundary:

```rust
#[derive(Debug, thiserror::Error)]
enum PathError {
    #[error("{0} is not a relative path")]
    NotRelative(String),
    #[error("cannot mix drives in {0!r} and {1!r}")]
    DriveMismatch(String, String),
}

impl From<PathError> for PyErr {
    fn from(e: PathError) -> PyErr {
        match e {
            PathError::NotRelative(_) | PathError::DriveMismatch(..) => {
                PyValueError::new_err(e.to_string())
            }
        }
    }
}
```

For error messages, we match CPython's exact wording where the test suite checks for it, and use clear descriptive messages elsewhere. The vendored test patches (section 6) handle cases where CPython's internal error formatting differs unavoidably.

### 4.8 Windows Path Parsing Details

Windows path parsing is implemented in pure Rust following PEP 428 and the NT kernel path spec. This means `PureWindowsPath` works identically on all platforms.

Path forms recognized:

| Form | Example | Parsed As |
|---|---|---|
| Local drive rooted | `C:\foo\bar` | `drive="C:"`, `root="\"`, parts: `["foo", "bar"]` |
| Local drive relative | `C:foo\bar` | `drive="C:"`, `root=None`, parts: `["foo", "bar"]` |
| UNC | `\\server\share\foo` | `drive="\\\\server\\share"`, `root="\"`, parts: `["foo"]` |
| Device | `\\.\C:\foo` | `drive="\\\\.\\C:"`, `root="\"`, parts: `["foo"]` |
| Extended-length | `\\?\C:\foo` | `drive="\\\\?\\C:"`, `root="\"`, parts: `["foo"]` |
| Extended UNC | `\\?\UNC\server\share\foo` | `drive="\\\\?\\UNC\\server\\share"`, `root="\"`, parts: `["foo"]` |

Key parsing rules:
- **Drive letter**: single ASCII letter followed by `:` at the start of the path
- **Root**: leading `\` (or `/`, normalized) after an optional drive
- **UNC**: exactly two leading backslashes followed by `server\share`
- **Extended-length prefix** (`\\?\`): treated as part of the drive, disables MAX_PATH limit in Win32 (informational in our parser)
- **Forward slash**: `/` is treated as a separator everywhere — Windows kernel accepts it

The parser normalizes separators to `\` for consistency with CPython's behavior, which reflects the canonical Windows form.

### 4.9 Thread Safety

The Rust core is thread-safe by design:

- `PathRepr` is `Send + Sync` — it contains only owned data (`OsString`) and a `OnceCell` (which is `Send + Sync` when the inner type is). No mutable shared state after construction.
- All operations on `&self` are read-only and can be called concurrently from multiple Python threads.
- IO operations (`stat`, `mkdir`, `unlink`, etc.) release the GIL before making OS calls, allowing other Python threads to run:
  ```rust
  fn stat(&self) -> PyResult<StatResult> {
      let path = self.inner.buf.clone();
      Python::with_gil(|py| {
          py.allow_threads(|| std::fs::metadata(&path))
      })
      .map_err(|e| PyErr::from(e))
  }
  ```
- The `OnceCell` for lazy parsing uses internal synchronization — concurrent first-time access from multiple threads is safe and only one parse occurs.
- Python-level: `PurePath` objects are immutable after construction and inherently thread-safe. `Path` objects are immutable handles to filesystem paths (filesystem state can change, but the `Path` object itself is immutable).

**Free-threading (Python 3.13+)**: PyO3 supports the free-threaded build via the `gil-refs` feature flag. The design above — releasing the GIL during IO, thread-safe internal state — is compatible with free-threading from the start.

### 4.10 Serialization Support

`pathlib.PurePath` is picklable through `__reduce__` (the path is just a string). Our implementation provides the same:

```rust
#[pymethods]
impl PurePath {
    fn __reduce__(&self, py: Python<'_>) -> PyResult<PyObject> {
        let cls = py.get_type::<Self>();
        // Return (cls, (str(self),)) — the same pickle format as CPython
        let args = (self.inner.buf.to_string_lossy().into_owned(),);
        Ok((cls, args).into_py(py))
    }

    fn __fspath__(&self) -> String {
        // OsStr → Python str. On Unix, OsStr is UTF-8 bytes (mandated by Python).
        // On Windows, OsStr is WTF-8; Python accepts this for __fspath__.
        self.inner.buf.to_string_lossy().into_owned()
    }
}
```

This means:
- `pickle.dumps(PurePosixPath("/a/b"))` works identically to CPython
- `copy.copy` and `copy.deepcopy` work via `__reduce__`
- `os.fspath()` returns the string representation
- Cross-process pickling works (the path string is portable)

`Path` objects (concrete paths with IO) are also just strings at the serialization level — the filesystem isn't part of the pickle state. This matches CPython behavior.

---

## 5. Memory Comparison

| Operation / Object | CPython | `pathlibrs` | Ratio |
|---|---|---|---|
| `PurePosixPath("/a/b/c/d.py")` | ~160 bytes | ~64 bytes | **2.5×** |
| Access `.parent` (first call) | allocates new str + PurePath | returns slice, no alloc | **instant** |
| Access `.suffix` | allocates str | returns slice, no alloc | **instant** |
| `p / "child"` | str concat + new PurePath | OsString reserve + push | **~2×** |
| `.stat()` | GIL + str-to-OsStr + syscall | direct syscall | comparable |
| `rglob("**/*.py")` on 10k files | huge list accumulation | bounded iterator | **depends** |

---

## 6. Testing Strategy — The Critical Part

The litmus test: **pass CPython's own `test_pathlib.py` unchanged.**

### Approach

1. **Vendored test suite**: Vendor a snapshot of CPython's `Lib/test/test_pathlib.py` (and supporting `test_support.py`) from Python 3.12 (our minimum supported version).

2. **Run against our module**: The tests import `pathlib` directly. We provide a test runner that:
   ```python
   import sys
   sys.modules['pathlib'] = __import__('pathlibrs')
   
   # Now run test_pathlib.py as-is
   ```

3. **CI gating**: Every CI run must execute the full vendored test suite. A regression in a test that previously passed is a blocker.

4. **Test-specific patches**: Some tests may probe CPython internals (`pathlib._flavour`, `pathlib._WIN`, exception message formatting). We maintain a small patch file for these with inline comments, and keep them as close to the original as possible.

5. **Coverage matrix**:
   - Linux (POSIX paths)
   - macOS (POSIX paths, case-insensitive FS)
   - Windows (via CI, Windows paths)
   - PureWindowsPath tests on Linux (ensuring Windows parsing works everywhere)

### Acceptance Criteria

- 100% of CPython's test_pathlib tests pass on the native platform
- 95%+ pass on cross-platform (e.g., PureWindowsPath on Linux)
- No behavioral differences for any documented API
- Any deviation is a bug, not a design choice

---

## 7. Implementation Phases

### Phase 1: Pure Paths (no IO) — ~2 weeks

- `PathRepr` struct with lazy parsing
- `PurePath`, `PurePosixPath`, `PureWindowsPath` as PyO3 classes
- Properties: `parts`, `drive`, `root`, `anchor`, `parent`, `parents`, `name`, `suffix`, `suffixes`, `stem`
- Methods: `joinpath()`, `with_name()`, `with_stem()`, `with_suffix()`, `relative_to()`, `is_relative_to()`, `as_posix()`, `as_uri()`
- Dunder: `__str__`, `__repr__`, `__fspath__`, `__eq__`, `__hash__`, `__lt__`
- `/` operator (`__truediv__`, `__rtruediv__`)
- `match()` (fnmatch-style pattern matching)
- **Verify:** PurePath tests pass

### Phase 2: Filesystem Properties — ~1 week

- `stat()`, `lstat()`, `exists()`, `is_dir()`, `is_file()`, `is_mount()`, `is_symlink()`, `is_junction()`
- `samefile()`, `owner()`, `group()`
- `resolve()`, `absolute()`, `readlink()`
- **Verify:** Filesystem property tests pass

### Phase 3: Filesystem Mutations & I/O — ~1 week

- `mkdir()`, `rmdir()`, `unlink()`, `rename()`, `replace()`, `symlink_to()`, `hardlink_to()`
- `touch()`, `chmod()`, `lchmod()`
- `open()`, `read_bytes()`, `read_text()`, `write_bytes()`, `write_text()`
- `iterdir()`, `glob()`, `rglob()`, `walk()` (3.12+)
- **Verify:** All mutation and glob tests pass

### Phase 4: Polish & Edge Cases — ~1 week

- `Path.home()`, `Path.cwd()` class methods
- `PurePath.with_segments()` class method (3.12+)
- Windows UNC/device/extended-path edge cases (see section 4.8)
- Symlink edge cases on Linux/macOS
- Pickle / `__reduce__` / `__fspath__` / `copy` support
- Full CPython test suite passes
- Benchmark suite against CPython pathlib

---

## 8. Benchmarks to Track

```python
# benchmark.py — run against both pathlib and pathlibrs

import pathlib     # standard lib
import pathlibrs   # our module
import tracemalloc

# Memory: count object sizes for 100k paths
# Speed: .parent, .suffix, .stem on 100k paths
# Speed: glob on directories of various depths
# Speed: walk on a large tree
# Speed: stat() on 10k files
```

Target benchmarks:

- `PurePath("/a/b/c/d/file.py").parent` — 10× faster (no allocation vs two allocations)
- `PurePath("/a/b/c/d/file.py").stem` — 10× faster (slice vs allocation)
- `p / "child"` — 3× faster (OsString prepend vs Python str concat + object creation)
- `PosixPath("/usr").resolve()` — comparable (syscall dominant)
- `p.rglob("**/*.py") on 10k files` — 2–5× less memory (iterator vs list)

---

## 9. Risks & Mitigations

| Risk | Mitigation |
|---|---|
| CPython test incompatibility with vendored `test_pathlib.py` | Pin a specific CPython tag; maintain a small patch file for CPython-internal probes |
| Windows path parsing on non-Windows hosts | Implement full Windows path parser in pure Rust using the spec from PEP 428 (section 4.8) |
| PyO3 subclassing complexity for 4-level hierarchy | Use `#[pyclass(subclass)]` and composition; avoid `extends` chain if possible |
| GIL contention on IO-heavy workloads | Release GIL during blocking IO calls (`stat`, `mkdir`, `walkdir`) — see section 4.9 |
| `pathlib.Path.open()` differing from `io.open()` | Delegate to Python's `io.open()` for full compatibility with all parameters (section 11.1) |
| CPython pathlib adds new features | Track CPython changelog; pin minimum semver compatibility |
| Pickle/copy incompatibility | Implement `__reduce__` returning `(cls, (str(path),))` — same as CPython (section 4.10) |

---

## 10. Project Layout

```
pathlibrs/
├── Cargo.toml
├── pyproject.toml          # maturin build config
├── src/
│   ├── lib.rs              # PyO3 module init, re-exports
│   ├── repr.rs             # PathRepr, ParsedPath
│   ├── parsing.rs          # parse_path(), drive/root extraction
│   ├── ops.rs              # stem, suffix, parent, etc. on &OsStr
│   ├── pattern.rs          # GlobPattern, fnmatch
│   ├── iter.rs             # parts, parents, glob iterators
│   ├── pure.rs             # PurePath / PurePosixPath / PureWindowsPath
│   ├── concrete.rs         # Path / PosixPath / WindowsPath
│   └── fs.rs               # stat, exists, mkdir, etc.
├── tests/
│   ├── test_pathlib.py     # Vendored from CPython
│   └── test_support.py     # Vendored support module
├── benchmarks/
│   ├── benchmark.py
│   └── fixtures/           # Test directory trees
└── README.md
```

---

## 11. Resolved Design Decisions

### 11.1 `Path.open()` — Delegate to Python's `io.open()`

**Decision**: Delegate to Python's `io.open()` via PyO3, not a native Rust file handle.

**Rationale**:
- `open()` has complex semantics: encoding negotiation, `newline` translation, `errors` handling, `buffering` modes, `opener` callbacks. Reimplementing these in Rust would be bug-prone and duplicative.
- Python callers often pass file objects to other Python code that expects `io.IOBase` subclasses (`TextIOWrapper`, `BufferedReader`). A Rust-backed file object wouldn't satisfy `isinstance(f, io.TextIOWrapper)` checks.
- CPython's own pathlib calls `io.open()` internally — we're matching the reference implementation.
- A Rust fast path for the common case (`open("rb")` without special flags) may be worth exploring later if benchmarks show a bottleneck.

```rust
#[pymethods]
impl Path {
    fn open(&self, py: Python<'_>,
            mode: &str, buffering: Option<isize>,
            encoding: Option<&str>, errors: Option<&str>,
            newline: Option<&str>, opener: Option<PyObject>)
        -> PyResult<PyObject>
    {
        let io = py.import("io")?;
        let kwargs = pyo3::types::PyDict::new(py);
        // ... set kwargs from parameters ...
        io.call_method("open", (self.inner.buf.as_os_str(),), Some(kwargs))
    }
}
```

### 11.2 Package Naming — `pathlibrs`

**Decision**: Ship as `pathlibrs`, an independent PyPI package.

**Rationale**:
- `pathlibrs` is descriptively clear ("pathlib in Rust") and doesn't collide with any existing package.
- `_pathlib` implies it's a private CPython implementation detail — it would conflict with the actual stdlib module and create confusion about who owns it.
- If CPython ever adopts this as the stdlib backend, the renaming to `_pathlib` is a trivial migration (the public API surface doesn't reference the module name).
- The package can be installed _alongside_ the standard library: `from pathlibrs import Path` coexists with `from pathlib import Path`. This is critical for gradual adoption and A/B testing.

### 11.3 Glob Ordering — Filesystem Order (No Guarantees)

**Decision**: Return results in filesystem order, matching CPython semantics.

**Rationale**:
- CPython's `pathlib.glob()` uses `os.scandir()`, which returns entries in filesystem-dependent order (typically inode order on Unix, alphabetical on NTFS). **Neither implementation guarantees any specific ordering.**
- The `walkdir` crate produces the same semantics.
- Users who need deterministic ordering should call `sorted()` on the result — this is already the documented recommendation for CPython.
- Adding mandatory sorting would hurt performance for the common case where order doesn't matter and would be a behavioral _difference_ from CPython, not a match.

### 11.4 Minimum Python Version — 3.12

**Decision**: Target Python 3.12+.

**Rationale**:
- Python 3.12 added `Path.walk()` and `PurePath.with_segments()` — both are referenced in this design and would need polyfill paths on older versions.
- Python 3.12 is the oldest Python version still receiving security fixes (support window: October 2023 – October 2028). Targeting it means we don't need to support EOL versions.
- PyO3's `abi3` feature for Python 3.12+ produces a single binary wheel that works across 3.12, 3.13, and future 3.x — simpler CI and distribution.
- Python 3.13's free-threading (no-GIL) is supported by our design but not required (see section 4.9).
- Python 3.12 adoption is high enough to justify the baseline; users on older versions can use stdlib `pathlib`.

### 11.5 Remaining Open Questions

These are deferred until the implementation yields data:

1. **Rust fast path for `open("rb")`**: If benchmarks show `io.open()` overhead is significant for simple binary open, add a native path that returns a `PyFile` wrapping a Rust `File`. Low priority — correctness first.

2. **`as_uri()` behavior on Windows**: CPython converts `PureWindowsPath("C:\\Users")` to `file:///C:/Users`. The spec (RFC 8089) is slightly ambiguous about drive-letter URIs. We'll match CPython's exact output via test-driven development.

3. **`expanduser()` on Windows**: Tilde expansion on Windows involves environment variables (`%USERPROFILE%`) and `HOME`/`HOMEDRIVE`/`HOMEPATH` fallbacks. This is a known-complex area; defer detailed design to implementation phase.
