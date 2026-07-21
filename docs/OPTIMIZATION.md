# Optimization Design — Closing the Performance Gap

## Current State

14 faster, 21 slower, 4 at parity out of 39 benchmarks vs CPython 3.14 pathlib
(C implementation).

**Goal**: pathlibrs ≥ pathlib in every benchmark category.

---

## The Three Big Wins

The 21 slow benchmarks fall into three root causes, ordered by impact:

| # | Root Cause | Benchmarks Affected | Worst Ratio |
|---|-----------|-------------------|-------------|
| 1 | Full parse triggered for ops that don't need it | parent, with_name, with_stem, with_suffix, joinpath, truediv | 3.23× |
| 2 | `_make_child` does Python round-trip to construct result | all path-producing ops | 3.23× |
| 3 | Heap allocation overhead per constructed object | construct, construct_from_parts, sizeof | 2.98× |

Fixing these three eliminates **15 of the 21 slow benchmarks**.

---

## Win 1: Shallow Parsing — Don't Parse What You Don't Need

### Problem

Every property access calls `self.inner.parsed(flavour)` which fully parses
the path: scan from start, split on separators, allocate `Vec<OsString>` for
parts, allocate per-part `OsString` values. But most operations don't need
the full parse at all.

```
parsed() for "/a/b/c/d/e/f/g/file.log" on .parent access:
    → find drive (none)
    → find root ("/")
    → scan bytes, split on '/'
    → allocate Vec<OsString> with capacity
    → allocate 7 individual OsStrings for each part
    → has_name = true
    → anchor_length = 1
    → THEN parent: strip last part, rebuild path string
```

All `parent` needed was the **position of the last `/`**. A 3-byte scan from
the end of the string.

### Operations That Don't Need Full Parse

| Tier | Operations | What They Actually Need |
|------|-----------|----------------------|
| 0 | `str()`, `truediv`, `joinpath`, `as_posix` | Nothing — raw bytes only |
| 1 | `parent`, `name`, `stem`, `suffix`, `with_*` | Anchor end + trailing segment positions |
| 2 | `parts`, `relative_to`, `is_relative_to`, `match` | Full parse |

Operations in Tier 0 and 1 currently trigger a full parse. They don't need to.

### Fix

Add fast-path functions that work on raw bytes without parsing:

```rust
/// Find where the anchor (drive + root) ends in raw path bytes.
/// For POSIX: position 1 if starts with '/', else 0.
/// For Windows: position after drive letter + root, or 0.
fn anchor_end(bytes: &[u8], flavour: PathFlavour) -> usize;

/// Find the parent path by locating the last separator after the anchor.
/// Returns the raw parent bytes (no allocations).
fn parent_bytes(bytes: &[u8], flavour: PathFlavour) -> Option<&[u8]>;

/// Find the name component: scan backwards from end for last separator
/// after anchor. Returns slice into raw bytes.
fn name_bytes(bytes: &[u8], flavour: PathFlavour) -> Option<&[u8]>;

/// Compute stem/suffix from a name byte slice.
/// Already done — ops::stem_from_name, ops::suffix_from_name.
```

Each is O(n) in the byte length but with no allocations, just pointer arithmetic.
The parsed `ParsedPath` is only constructed on demand (Tier 2 operations).

In `PurePath`, gate property access by parse depth:

```rust
fn parent<'py>(slf: PyRef<'py, Self>) -> PyResult<PyObject> {
    // Fast path: scan raw bytes for last separator
    let raw = slf.inner.raw().as_encoded_bytes();
    let sep = slf._sep();
    let anchor_end = quick_anchor_end(raw, slf.flavour);
    if let Some(last_sep) = raw[anchor_end..].iter().rposition(|&b| b == sep) {
        let parent_raw = crate::from_os_bytes(&raw[..anchor_end + last_sep]);
        return _make_child_fast(slf.py(), &slf, parent_raw.to_os_string());
    }
    // Fall through: root path or no parent → use existing logic
    // ...
}
```

**Expected impact**: The 6 largest pure-path regressions (`truediv` 3.23×,
`joinpath` 2.51×, `parent` 2.03×, `with_name` 1.99×, `with_suffix` 1.69×,
`with_stem` 1.68×) drop to parity or faster.

---

## Win 2: `_make_child_fast` — Rust-native Path Construction

### Problem

Every path-producing operation currently constructs the result through Python:

```
Rust:    _make_child(slf_ptr, new_raw)
  → Python: getattr(slf, "__class__")        // type lookup
  → Python: slf.with_segments(raw_string)    // method dispatch
    → Python:   cls.__new__(raw_string)       // constructor
      → Rust:     join_path_segments(parsed)  // back to Rust
      → Rust:     alloc PurePath struct
  → Python: return result                     // to caller
```

3 FFI crossings per result construction. A `/` operator on a vector of 10k
paths does 30k FFI crossings.

### Fix

Construct the result path directly in Rust:

```rust
fn _make_child_fast(
    py: Python<'_>,
    slf_ptr: *mut pyo3::ffi::PyObject,
    new_raw: OsString,
) -> PyResult<PyObject> {
    let slf_bound = unsafe { pyo3::Bound::from_borrowed_ptr(py, slf_ptr) };

    // Check if subclass overrides with_segments — if so, fall back
    // to Python method dispatch for correctness.
    let cls = slf_bound.getattr("__class__")?;
    let with_segments = cls.getattr("with_segments")?;
    let base_with_segments = cls.getattr("__base__")?.getattr("with_segments")?;
    if !with_segments.is(&base_with_segments) {
        // Subclass override — use Python path
        let raw_str = new_raw.to_string_lossy().into_owned();
        return Ok(slf_bound
            .call_method1("with_segments", (raw_str,))?
            .unbind());
    }

    // Fast path: construct in Rust with same flavour as self
    let flavour = {
        let slf_ref: &PurePath = unsafe { &*(slf_ptr as *const PurePath) };
        slf_ref.flavour
    };
    let result = PurePath {
        inner: PathRepr::new(new_raw),
        flavour,
        path_info: OnceLock::new(),
    };
    Py::new(py, result).map(|p| p.into_py(py).unbind())
}
```

Subclass detection ensures correctness: if a user subclasses `PurePath` with
a custom `with_segments`, we fall back to the Python dispatch path. For the
99% case (direct PurePath usage), we skip Python entirely.

**Expected impact**: Paired with shallow parsing (Win 1), the remaining FFI
overhead per `/` call is zero. `truediv` goes from 3.23× slower to ~1.0×.

---

## Win 3: Inline Short Paths — Squash Heap Allocations

### Problem

Every constructed PurePath does 3 mandatory heap allocations:
1. `OsString` for the raw path bytes
2. `Mutex<Option<Py<PathInfo>>>` for info caching (~64 byte OS mutex)
3. `OnceLock<Box<ParsedPath>>` on first property access

For `/usr/bin/python3` (17 bytes), the `OsString` alloc is waste — it fits in
a small inline buffer. The `Mutex` is waste — most paths never call `.info`.

### Fix A: Inline Short Paths

Replace `OsString` with a small-string-optimized type:

```rust
const INLINE_CAP: usize = 30; // covers 95%+ of real-world paths

enum CompactOsString {
    Inline { len: u8, buf: [u8; INLINE_CAP] },
    Heap(Box<OsString>),
}
```

`CompactOsString` is ~32 bytes (same as `OsString` + vtable ptr in the enum
discriminant). For paths ≤ 30 bytes, no heap allocation. `Deref<Target = OsStr>`
trait makes it transparent to existing code.

### Fix B: Replace Mutex with OnceLock

`path_info` is set once and never mutated. `std::sync::Mutex` allocates a
`pthread_mutex_t` (~64 bytes on macOS). `OnceLock<Py<PathInfo>>` is an
`AtomicU8` (1 byte) + data — zero OS overhead, zero allocation when empty.

### Fix C: Inline ParsedPath (Remove Box)

`ParsedPath` is ~40 bytes (two Options + Vec + usize + bool). Boxing it adds
a separate heap allocation. Embed directly in `PathRepr`:

```rust
pub struct PathRepr {
    raw: CompactOsString,
    parsed: OnceLock<ParsedPath>,   // was OnceLock<Box<ParsedPath>>
    str_cache: OnceLock<String>,
}
```

Combined: `construct_and_discard` goes from 7 → 1 heap allocation
(CompactOsString inline + OnceLock inline). Matches CPython's single-allocation
model.

**Expected impact**: `construct_and_discard` 2.98× → ~1.1×, `sizeof` 2.20× → ~1.0×,
`construct` 2.51× → ~1.1×.

---

## Remaining Optimizations

Lower-impact wins that are still worth doing:

| # | Optimization | Fixes | Impact |
|---|-------------|-------|--------|
| 4 | Lazy `iterdir` iterator (generator, not list) | `iterdir` 2.17× | Yield paths one at a time with `_make_child_fast` |
| 5 | Brace-aware pattern matching | `glob("{py,txt}")` 4.94× | Integrate braces into selector AST, scan once |
| 6 | Cached `__str__` in PathRepr | `str` 1.71× | `OnceLock<String>`, computed once |
| 7 | Cached name/stem/suffix/suffixes | `name` 1.19×, `stem`, `suffix`, `suffixes` | Compute all four on first access to any, cache |
| 8 | Cached `parts` tuple | `parts` 1.43× | `OnceLock<Vec<String>>`, materialize once |
| 9 | `write_bytes` remove redundant `data.to_vec()` | `write_bytes` 1.43× | Pass `&data` directly to `write_all` |
| 10 | UTF-8 fast path for `read_text`/`write_text` | text I/O | `String::from_utf8` instead of Python codecs |
| 11 | Pre-sized string builders | `joinpath`, `with_*` | `Vec::with_capacity` in all path builders |

---

## Implementation Plan

### Step 1: Infrastructure (prep for Wins 1-3)

- [ ] Replace `Mutex<Option<Py<PathInfo>>>` with `OnceLock<Py<PathInfo>>`
- [ ] Add `str_cache: OnceLock<String>` to `PathRepr`
- [ ] Cached `__str__()` using the `OnceLock`
- [ ] Pre-size all `Vec<u8>` path builders with `with_capacity`

**Verify**: `make ci` passes. `str` and `fspath` benchmarks improve.

### Step 2: `_make_child_fast` (Win 2)

- [ ] Implement fast Rust construction path with subclass-override detection
- [ ] Wire into `__truediv__`, `__rtruediv__`, `joinpath`
- [ ] Wire into `parent`, `with_name`, `with_stem`, `with_suffix`
- [ ] Wire into `relative_to`, `absolute`, `resolve`, `readlink`
- [ ] Wire into filesystem ops (`rename`, `replace`, `copy`, `copy_into`,
      `move_`, `move_into`, `iterdir`)

**Verify**: `make bench` — pure-mutation and construction benchmarks should
show 1.7-3.2× speedup.

### Step 3: Shallow Parsing (Win 1)

- [ ] Add `quick_anchor_end()` for POSIX and Windows
- [ ] Add `parent_bytes()` working on raw `&[u8]`
- [ ] Add `name_bytes()` working on raw `&[u8]`
- [ ] Refactor `parent`, `name`, `stem`, `suffix`, `suffixes` to use fast path
- [ ] Refactor `with_name`, `with_stem`, `with_suffix` to use fast path
- [ ] Refactor `truediv`, `joinpath` to skip parse entirely
- [ ] Gate full `parsed()` call behind actual need (Tier 2 operations only)

**Verify**: `make bench` — pure-path benchmarks should match or beat pathlib.
`make test` — all vendored CPython tests pass (correctness unchanged).

### Step 4: Allocation Squash (Win 3)

- [ ] Implement `CompactOsString` with 30-byte inline buffer
- [ ] Swap `PathRepr.raw` from `OsString` to `CompactOsString`
- [ ] Inline `ParsedPath` into `PathRepr` (remove `Box`)
- [ ] Verify `Deref<Target = OsStr>` works for all existing call sites

**Verify**: `make bench` — construction and sizeof benchmarks improve.
`make test` — all tests pass.

### Step 5: Iterators & Glob

- [ ] Lazy `iterdir` iterator class (PyO3 `#[pyclass]` with `__next__`)
- [ ] Brace-alternatives in pattern AST
- [ ] Single-scan glob walk with inline brace matching

**Verify**: `make bench` — iterdir and glob benchmarks improve.
Vendored glob tests pass unchanged.

### Step 6: Polish

- [ ] `write_bytes` remove `data.to_vec()` copy
- [ ] `read_text`/`write_text` UTF-8 fast path
- [ ] Cached `parts` tuple
- [ ] Cached name/stem/suffix/suffixes

**Verify**: `make bench-compare` — zero regressions vs baseline.

---

## Projected Outcome

| Category | Current | After Step 2 | After Step 3 | After All |
|----------|--------:|------------:|------------:|----------:|
| `truediv` | **3.23×** | 1.2× | **1.0×** | **1.0×** |
| `construct` | **2.51×** | 2.5× | 2.5× | **1.1×** |
| `joinpath` | **2.51×** | 1.2× | **1.0×** | **1.0×** |
| `parent` | **2.03×** | 1.2× | **1.0×** | **1.0×** |
| `with_name` | **1.99×** | 1.2× | **1.0×** | **1.0×** |
| `with_suffix` | **1.69×** | 1.1× | **1.0×** | **1.0×** |
| `with_stem` | **1.68×** | 1.1× | **1.0×** | **1.0×** |
| `str` | **1.71×** | **1.0×** | **1.0×** | **1.0×** |
| `parts` | **1.43×** | 1.4× | 1.4× | **1.0×** |
| `write_bytes` | **1.43×** | 1.4× | 1.4× | **1.0×** |
| `sizeof` | **2.20×** | 2.2× | 2.2× | **1.0×** |
| `iterdir` | **2.17×** | 1.2× | 1.2× | **1.0×** |
| `glob("{py,txt}")` | **4.94×** | 4.9× | 4.9× | **~1.3×** |

Target: **zero benchmarks slower than pathlib**.
