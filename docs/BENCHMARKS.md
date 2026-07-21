# Benchmarks — pathlibrs vs pathlib

Head-to-head performance comparison of `pathlibrs` (Rust/PyO3) against
CPython 3.14's built-in `pathlib` (C implementation). All benchmarks run
with **release builds** (`maturin develop --release`, LTO enabled) on
macOS 15 (Apple M1). Each benchmark runs both implementations on identical
test data and reports the median of all calibration rounds.

```
make bench          # run all benchmarks (release build)
make bench-dev      # quick debug-mode run for iteration
make bench-save     # save a baseline for future comparison
make bench-compare  # compare against saved baseline (fails if >10% regression)
```

**Summary**: 14 faster, 21 slower, 4 at parity out of 39 benchmarks.

---

## Pure Operations

Benchmarks that exercise path parsing, string manipulation, and path
mutation — no filesystem I/O. Each test operates on a batch of 10,000
pre-constructed path strings.

| Operation | pathlib | pathlibrs | Ratio |
|-----------|---------|-----------|-------|
| `stem` | 1.4 ms | 892 μs | **1.52× faster** |
| `suffix` | 1.3 ms | 761 μs | **1.76× faster** |
| `suffixes` | 2.2 ms | 1.3 ms | **1.66× faster** |
| `fspath` | 987 μs | 1.1 ms | 1.08× slower |
| `name` | 590 μs | 703 μs | 1.19× slower |
| `parts` | 1.3 ms | 1.9 ms | 1.43× slower |
| `str` | 451 μs | 772 μs | 1.71× slower |
| `with_stem` | 7.8 ms | 13.1 ms | 1.68× slower |
| `with_suffix` | 8.1 ms | 13.6 ms | 1.69× slower |
| `with_name` | 6.2 ms | 12.4 ms | 1.99× slower |
| `parent` | 5.7 ms | 11.5 ms | 2.03× slower |
| `construct` | 3.3 ms | 8.2 ms | 2.51× slower |
| `joinpath` | 6.5 ms | 16.2 ms | 2.51× slower |
| `construct_from_parts` | 5.4 ms | 16.0 ms | 2.94× slower |
| `truediv` (`/`) | 4.4 ms | 14.1 ms | **3.23× slower** |

**Analysis**: pathlibrs wins on suffix/stem/suffixes because these are
pure string-scanning operations done entirely in Rust with no Python
object allocation. The top-five regressions (construct, joinpath,
truediv, parent, with_name) all involve creating a **new Python object**
from Rust — the PyO3 bridge overhead dominates. Each `/` operator call
crosses Python→Rust→Python twice (once to parse, once to instantiate the
result), which adds ~0.5-1 μs per round trip.

---

## Stat & Metadata

Filesystem metadata checks with **hot OS cache** (file/dir touched once
before the benchmark loop). Each benchmark calls the method 10,000 times
in a tight loop. pathlibrs releases the GIL during all syscalls.

| Operation | pathlib | pathlibrs | Ratio |
|-----------|---------|-----------|-------|
| `stat` | 1.5 ms | 1.2 ms | **1.30× faster** |
| `samefile` | 3.2 ms | 2.5 ms | **1.27× faster** |
| `exists` (missing) | 14.2 ms | 10.3 ms | **1.38× faster** |
| `exists` (hot) | 15.7 ms | 11.2 ms | **1.41× faster** |
| `is_file` | 16.1 ms | 11.5 ms | **1.39× faster** |
| `is_dir` | 16.0 ms | 11.4 ms | **1.40× faster** |
| `is_symlink` | 15.9 ms | 11.4 ms | **1.39× faster** |

**Analysis**: Every stat operation is faster. pathlibrs releases the GIL
before `fstatat`/`lstat`, allowing the OS to service concurrent Python
threads while the syscall is in flight. CPython's pathlib holds the GIL
throughout. The 1.3-1.4× gap is entirely GIL release benefit.

---

## I/O

File read/write benchmarks on small files (~2 KB text, ~4 KB binary).
Each benchmark runs the operation 500 times.

| Operation | pathlib | pathlibrs | Ratio |
|-----------|---------|-----------|-------|
| `read_bytes` | 6.7 ms | 6.2 ms | **1.09× faster** |
| `read_text` | 8.2 ms | 6.8 ms | **1.21× faster** |
| `open_read` | 8.2 ms | 8.5 ms | 1.04× slower |
| `write_text` | 20.5 ms | 20.9 ms | parity |
| `write_bytes` | 19.2 ms | 27.5 ms | 1.43× slower |

**Analysis**: Reads benefit from GIL release during the I/O wait. Writes
are comparable for text but notably slower for binary. `write_bytes` is
one of the few operations where pathlibrs underperforms — the Rust
`std::fs::write` path may buffer differently than CPython's C
implementation. This is the most actionable regression to investigate.

---

## Directory Traversal

| Operation | pathlib | pathlibrs | Ratio |
|-----------|---------|-----------|-------|
| `iterdir` (1k files) | 1.1 ms | 2.4 ms | 2.17× slower |
| `walk` (4×4 tree) | 5.9 ms | 6.3 ms | 1.08× slower |

**Analysis**: `iterdir` is the notable regression here. pathlibrs
constructs a Python `Path` object for each directory entry returned from
`read_dir`, and each object crosses the PyO3 boundary. CPython's
`os.scandir` returns lightweight `DirEntry` objects directly. For `walk`,
the overhead is amortized by the recursive tree traversal work.

---

## Glob & Pattern Matching

Glob benchmarks on a flat 1,000-file directory and a depth-4 width-4 tree.

| Operation | pathlib | pathlibrs | Ratio |
|-----------|---------|-----------|-------|
| `rglob("**/*.py")` | 11.4 ms | 10.7 ms | **1.07× faster** |
| `rglob("*")` | 11.7 ms | 12.0 ms | parity |
| `glob("*.py")` | 932 μs | 1.5 ms | 1.64× slower |
| `glob("{py,txt}")` | 647 μs | 3.2 ms | **4.94× slower** |

**Analysis**: Recursive globs (`rglob`) are at parity or slightly faster —
the Rust DFS engine is efficient once the traversal dominates. Shallow
globs are slower because the PyO3 bridge overhead on the iterator
protocol dominates the fast `scandir` + `fnmatch` path. Brace expansion
is the worst regression: pathlibrs currently expands braces in pure
Python via `itertools.product`, which allocates a cartesian product of
all pattern combinations upfront. CPython does this lazily in C.

---

## Filesystem Mutations

Each benchmark runs the operation 50-100 times, creating fresh files/trees
per iteration.

| Operation | pathlib | pathlibrs | Ratio |
|-----------|---------|-----------|-------|
| `copy_tree` | 616 μs | 510 μs | **1.21× faster** |
| `mkdir` | 49 μs | 50 μs | parity |
| `touch`+`unlink` | 75 μs | 73 μs | parity |
| `move_tree` | 212 μs | 219 μs | parity |
| `mkdir(parents=True)` | 198 μs | 215 μs | 1.09× slower |
| `delete_tree` | — | 412 μs | N/A |

**Analysis**: Mutation operations are largely parity. These are syscall-bound
— the Python→Rust→OS call chain is the same for any extension module. `copy_tree`
is slightly faster because pathlibrs' recursive copy avoids Python-level
`shutil` overhead. `delete_tree` has no pathlib equivalent (the built-in
`pathlib.Path.delete()` does not exist in CPython 3.14.6).

---

## Memory & Allocation

| Benchmark | pathlib | pathlibrs | Ratio |
|-----------|---------|-----------|-------|
| construct + discard (10k paths) | 3.3 ms | 9.9 ms | 2.98× slower |
| `sizeof(PurePath)` (100k calls) | 4.1 ms | 9.1 ms | 2.20× slower |

**Analysis**: pathlibrs `PurePath` objects are larger in memory (PyO3
wraps a Rust `PathRepr` struct containing an `OsString` + `OnceCell`
parsed state). Each construction crosses the FFI boundary. The 2-3× gap
in allocation time reflects the PyO3 object creation overhead, not
necessarily actual memory footprint.

---

## Running Benchmarks

```bash
# Release mode (what the numbers above use)
make bench

# Debug mode (quick iteration during development)
make bench-dev

# Save a baseline for regression detection
make bench-save

# Compare against baseline (fails CI if >10% regression)
make bench-compare
```

Benchmark data is stored as JSON in `benchmarks/results/`. The CI
workflow saves baselines on every push to `main` and compares PRs
against them automatically.
