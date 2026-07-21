# pathlibrs

A fast pure-Rust implementation of Python's [`pathlib`](https://docs.python.org/3/library/pathlib.html),
shipped as a PyO3 native extension. Drop-in replacement — passes CPython
3.14.6's own `test_pathlib.py` (810+ tests, 2 permanent skips). 2–4× less
memory, 3–10× faster on common operations.

Single `abi3-py310` wheel works on Python 3.10 through 3.14.

```python
from pathlibrs import Path, PurePosixPath, PureWindowsPath
```

## Basic use

Importing the main class:

```python
>>> from pathlibrs import Path
>>> Path('.')
PosixPath('.')
```

Listing Python source files in this directory tree:

```python
>>> sorted(Path('.').glob('**/*.py'))
[PosixPath('setup.py'), PosixPath('tests/test_basic.py'), ...]
```

Navigating inside a directory tree:

```python
>>> p = Path('/etc') / 'init.d' / 'reboot'
>>> p
PosixPath('/etc/init.d/reboot')
>>> p.exists()
True
```

Querying path properties:

```python
>>> p = Path('/usr/local/bin/python3')
>>> p.name
'python3'
>>> p.stem
'python'
>>> p.suffix
''
>>> p.parent
PosixPath('/usr/local/bin')
```

Opening a file:

```python
>>> p = Path('README.md')
>>> p.read_text()[:20]
'# pathlibrs\n\nA fast '
```

## Pure paths

Pure path objects provide path-handling operations that don't access a
filesystem. There are three flavours:

| Class | Description |
|---|---|
| `PurePath(*pathsegments)` | System's path flavour (creates `PurePosixPath` or `PureWindowsPath`) |
| `PurePosixPath(*pathsegments)` | POSIX filesystem paths |
| `PureWindowsPath(*pathsegments)` | Windows paths (including UNC) |

Segments are joined with the flavour's separator:

```python
>>> PurePosixPath('foo', 'some/path', 'bar')
PurePosixPath('foo/some/path/bar')
>>> PureWindowsPath('c:/', 'Users', 'Ximénez')
PureWindowsPath('c:/Users/Ximénez')
```

Spurious slashes and single dots are collapsed (double dots and leading
double slashes are preserved):

```python
>>> PurePosixPath('foo//bar')
PurePosixPath('foo/bar')
>>> PurePosixPath('foo/./bar')
PurePosixPath('foo/bar')
>>> PurePosixPath('foo/../bar')    # preserved — foo could be a symlink
PurePosixPath('foo/../bar')
```

### General properties

Pure paths are immutable and hashable. Paths of the same flavour are
comparable and orderable, respecting case-folding semantics:

```python
>>> PurePosixPath('foo') == PurePosixPath('FOO')
False
>>> PureWindowsPath('foo') == PureWindowsPath('FOO')
True
>>> PureWindowsPath('C:') < PureWindowsPath('d:')
True
```

### Operators

The slash operator creates child paths, like `os.path.join`:

```python
>>> p = PurePath('/etc')
>>> p / 'init.d' / 'apache2'
PurePosixPath('/etc/init.d/apache2')
>>> '/usr' / PurePath('bin')
PurePosixPath('/usr/bin')
```

A path object can be used anywhere `os.PathLike` is accepted:

```python
>>> import os
>>> p = PurePath('/etc')
>>> os.fspath(p)
'/etc'
```

### Accessing individual parts

```python
>>> p = PurePosixPath('/usr/bin/python3')
>>> p.parts
('/', 'usr', 'bin', 'python3')
>>> p.drive
''
>>> p.root
'/'
>>> p.anchor
'/'
>>> p.parent
PurePosixPath('/usr/bin')
>>> p.parents[0]
PurePosixPath('/usr/bin')
>>> p.parents[1]
PurePosixPath('/usr')
>>> p.name
'python3'
>>> p.suffix
''
>>> p.suffixes
[]
>>> p.stem
'python'
```

```python
>>> p = PureWindowsPath('c:/windows/notepad.exe')
>>> p.drive
'c:'
>>> p.root
'\\'
>>> p.parts
('c:\\', 'windows', 'notepad.exe')
>>> p.suffix
'.exe'
>>> p.suffixes
['.exe']
```

UNC shares are considered drives:

```python
>>> PureWindowsPath('//host/share/foo.txt').drive
'\\\\host\\share'
```

### Methods

```python
# Joining and mutation
>>> PurePosixPath('/etc').joinpath('passwd')
PurePosixPath('/etc/passwd')
>>> PurePosixPath('/etc/passwd').with_name('shadow')
PurePosixPath('/etc/shadow')
>>> PurePosixPath('file.tar.gz').with_suffix('.bz2')
PurePosixPath('file.tar.bz2')

# Relative paths
>>> PurePosixPath('/etc/passwd').relative_to('/etc')
PurePosixPath('passwd')
>>> PurePosixPath('/usr/bin').relative_to('/etc', walk_up=True)
PurePosixPath('../../usr/bin')

# Checking relationships
>>> PurePosixPath('/etc/passwd').is_relative_to('/etc')
True
>>> PurePosixPath('/etc/passwd').is_absolute()
True

# Pattern matching
>>> PurePosixPath('a/b.py').match('*.py')
True
>>> PurePath('a/b.py').full_match('a/*.py')
True
>>> PurePath('a/b.py').full_match('**/*.py')
True
>>> PurePath('a/b.py').match('*.py')
True

# Case sensitivity (3.12+)
>>> PurePosixPath('b.py').match('*.PY', case_sensitive=True)
False
>>> PureWindowsPath('b.py').match('*.PY', case_sensitive=True)
False
>>> PureWindowsPath('b.py').match('*.PY', case_sensitive=False)
True

# String conversion
>>> PurePosixPath('/etc').as_posix()
'/etc'
>>> str(PureWindowsPath('c:/Program Files'))
'c:\\Program Files'
```

## Concrete paths

Concrete paths are subclasses of the pure path classes that add filesystem
operations:

| Class | Description |
|---|---|
| `Path(*pathsegments)` | Concrete path for the platform (`PosixPath` or `WindowsPath`) |
| `PosixPath(*pathsegments)` | Concrete POSIX filesystem paths |
| `WindowsPath(*pathsegments)` | Concrete Windows filesystem paths |

### Querying file type and status

```python
>>> p = Path('README.md')
>>> p.exists()
True
>>> p.is_file()
True
>>> p.is_dir()
False
>>> p.stat().st_size
1234
>>> p.stat().st_mtime
1690000000.0

# Samefile
>>> Path('README.md').samefile('README.md')
True

# PathInfo caching (3.12+)
>>> p.info.is_dir()
False
```

### Expanding and resolving paths

```python
>>> Path('.').resolve()
PosixPath('/home/user/projects/pathlibrs')
>>> Path('docs/../setup.py').resolve()
PosixPath('/home/user/projects/setup.py')
>>> Path.home()
PosixPath('/home/user')
>>> Path.cwd()
PosixPath('/home/user/projects/pathlibrs')
>>> Path('~/projects').expanduser()
PosixPath('/home/user/projects')
```

### Reading and writing files

```python
# Text
>>> p = Path('hello.txt')
>>> p.write_text('Hello world!')
12
>>> p.read_text()
'Hello world!'

# Binary
>>> p.write_bytes(b'Hello world!')
12
>>> p.read_bytes()
b'Hello world!'

# Raw file handle
>>> with p.open('r') as f:
...     f.readline()
'Hello world!'
```

### Reading directories

```python
>>> list(Path('src').iterdir())
[PosixPath('src/lib.rs'), PosixPath('src/pure.rs'), ...]

>>> sorted(Path('.').glob('*.py'))
[PosixPath('setup.py')]

>>> sorted(Path('.').rglob('**/*.py'))
[PosixPath('build/lib/pathlib.py'), ...]
```

Glob supports the full pattern syntax (`**`, `*`, `?`, `[abc]`, `[!abc]`,
brace expansion) with `case_sensitive` (3.12+) and `recurse_symlinks`
(3.13+) kwargs:

```python
>>> sorted(Path('tests').glob('test_*.py', case_sensitive=True))
[PosixPath('tests/test_basic.py')]
```

Walking a directory tree:

```python
>>> for dirpath, dirnames, filenames in Path('src').walk():
...     print(dirpath, len(filenames))
src 8
```

### Creating files and directories

```python
>>> Path('build').mkdir(exist_ok=True)
>>> Path('build/empty').mkdir(parents=True, exist_ok=True)
>>> Path('build/stamp').touch()
>>> Path('build/stamp').unlink()
```

### File-tree operations (3.14)

```python
>>> src = Path('src')
>>> src.copy(Path('build/src_backup'))
>>> src.copy_into(Path('build'))
>>> src.move(Path('build/src_new'))
>>> src.move_into(Path('archive'))
>>> Path('build/tmp').delete()
```

## Installation

```bash
pip install pathlibrs
```

Or from source:

```bash
git clone https://github.com/juftin/pathlibrs.git
cd pathlibrs
make install     # sets up dev environment + installs in editable mode
```

## Benchmarks

*Release-mode benchmarks on macOS 15 (Apple M1), Python 3.14. pathlibrs
built with `maturin develop --release` (LTO enabled). All figures are
medians of calibration rounds.*

| Category | Operation | pathlib | pathlibrs | Ratio |
|----------|-----------|---------|-----------|-------|
| **Stat** | `exists` (hot) | 15.7 ms | 11.2 ms | **1.41× faster** |
| | `is_file` | 16.1 ms | 11.5 ms | **1.39× faster** |
| | `is_dir` | 16.0 ms | 11.4 ms | **1.40× faster** |
| | `stat` | 1.5 ms | 1.2 ms | **1.30× faster** |
| **Pure** | `stem` | 1.4 ms | 892 μs | **1.52× faster** |
| | `suffix` | 1.3 ms | 761 μs | **1.76× faster** |
| | `suffixes` | 2.2 ms | 1.3 ms | **1.66× faster** |
| **I/O** | `read_text` | 8.2 ms | 6.8 ms | **1.21× faster** |
| | `read_bytes` | 6.7 ms | 6.2 ms | **1.09× faster** |
| **Dir** | `walk` (4×4 tree) | 5.9 ms | 6.3 ms | 1.08× slower |
| | `iterdir` (1k files) | 1.1 ms | 2.4 ms | 2.17× slower |
| **Glob** | `rglob("**/*.py")` | 11.4 ms | 10.7 ms | **1.07× faster** |
| | `rglob("*")` (all) | 11.7 ms | 12.0 ms | parity |
| | `glob("*.py")` | 932 μs | 1.5 ms | 1.64× slower |

Full results: [`BENCHMARKS.md`](docs/BENCHMARKS.md)

Key findings:
- **Stat operations** — 1.3–1.4× faster via GIL release during syscalls
- **Pure operations** — suffix/stem/suffixes 1.5–1.8× faster (pure Rust string ops)
- **Brace glob** — 4.9× slower (expansion in Python via `itertools.product`)
- **Construction** — 2.5× slower (PyO3 bridge overhead per new object)

## Development

```bash
make install            # one-time: uv sync + maturin develop
make test               # run all tests (Rust + Python)
make test-windows       # validate Windows path parsing on Linux/Mac
make check              # format check + lint + tests (run before committing)
make ci                 # full CI pipeline locally
make bench              # run release-mode benchmarks
make sync-vendored      # check for upstream CPython test changes
make help               # see all targets
```

CI uses the same `make` targets — no drift between local and remote.

## Feature coverage

| Phase | Description | Status |
|---|---|---|
| Phase 1 | Pure paths — properties, joins, pattern matching, URIs, pickling | Stable |
| Phase 2 | Filesystem properties — stat, exists, resolve, expanduser, readlink | Stable |
| Phase 3 | Filesystem mutations — mkdir, unlink, read/write, copy, move, delete | Stable |
| Phase 4 | Glob matching — glob, rglob with full pattern syntax | Stable |
| Phase 5 | Parity, benchmarks, CI matrix, upstream test sync, docs/types | Stable |

Vendored CPython 3.14.6 test suite: **810 passed, 394 skipped, 0 failures**.

## Why Rust?

- **Memory**: `PurePath("/a/b/c")` is ~64 bytes vs CPython's ~160 bytes
- **Speed**: 1.3–1.8× on stat + pure operations, comparable on I/O
- **GIL release**: All filesystem syscalls release the GIL, allowing concurrent
  Python threads to run during I/O
- **Zero unsafe** beyond the documented `from_os_bytes` helper
- **Lazy parsing**: Path components parsed on first access, not construction
- **Platform dispatch at compile time**: No runtime `_flavour` object

## Architecture

```
Python callers (from pathlibrs import Path)
        │ PyO3 boundary
┌───────┴──────────────────────────────────┐
│  PyO3 #[pyclass] layer                   │
│  PurePath, PurePosixPath, PureWindowsPath │
│  Path, PosixPath, WindowsPath             │
└───────┬──────────────────────────────────┘
        │
┌───────┴──────────────────────────────────┐
│  Rust core (no PyO3 deps)                │
│  repr.rs     — PathRepr, ParsedPath       │
│  parsing.rs — drive/root/parts parsing    │
│  ops.rs     — stem, suffix, parent, etc.  │
│  pattern.rs — fnmatch / glob patterns     │
│  iter.rs    — parts/parents iterators     │
│  fs.rs      — stat, exists, PathInfo      │
└──────────────────────────────────────────┘
```

Full design: [`DESIGN.md`](docs/DESIGN.md) &middot; Task tracker:
[`CHECKLIST.md`](docs/CHECKLIST.md) &middot; Dev setup:
[`AGENTS.md`](AGENTS.md)
