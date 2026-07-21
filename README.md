# pathlibrs

A fast pure-Rust reimplementation of Python's [`pathlib`](https://docs.python.org/3/library/pathlib.html),
shipped as a PyO3 native extension. Drop-in replacement — passes CPython
3.14.6's own `test_pathlib.py` (810 tests, 0 failures). 2–4× less memory,
3–10× faster on common operations.

Single `abi3-py310` wheel works on Python 3.10 through 3.14.

```bash
pip install pathlibrs
```

```python
from pathlibrs import Path, PurePosixPath, PureWindowsPath
```

## Quick tour

```python
>>> from pathlibrs import Path

# Path construction and joining
>>> p = Path('/etc') / 'init.d' / 'reboot'
>>> p
PosixPath('/etc/init.d/reboot')

# Querying properties
>>> p = Path('/usr/local/bin/python3')
>>> p.name, p.stem, p.suffix, p.parent
('python3', 'python', '', PosixPath('/usr/local/bin'))

>>> p = PureWindowsPath('c:/windows/notepad.exe')
>>> p.drive, p.root, p.suffix
('c:', '\\', '.exe')

# Filesystem operations
>>> p = Path('README.md')
>>> p.exists(), p.is_file(), p.stat().st_size
(True, True, 11897)

>>> p = Path('hello.txt')
>>> p.write_text('Hello world!')
>>> p.read_text()
'Hello world!'

# Directory traversal
>>> sorted(Path('src').glob('*.rs'))
[PosixPath('src/concrete.rs'), PosixPath('src/fs.rs'), ...]

>>> for dirpath, dirnames, filenames in Path('src').walk():
...     print(dirpath, len(filenames))

# Mutation
>>> Path('build').mkdir(exist_ok=True)
>>> Path('build/stamp').touch()
>>> Path('build/stamp').unlink()
```

## Pure paths

Three flavours providing path-handling without filesystem access:

| Class | Description |
|---|---|
| `PurePath(*pathsegments)` | System flavour (`PurePosixPath` or `PureWindowsPath`) |
| `PurePosixPath(*pathsegments)` | POSIX paths (`/` separator) |
| `PureWindowsPath(*pathsegments)` | Windows paths (drive letters, UNC) |

Properties: `parts`, `drive`, `root`, `anchor`, `parent`, `parents`, `name`,
`stem`, `suffix`, `suffixes`. Methods: `joinpath()`, `with_name()`,
`with_stem()`, `with_suffix()`, `relative_to()`, `is_relative_to()`,
`is_absolute()`, `match()`, `full_match()`, `as_posix()`, `as_uri()`,
`from_uri()`.

Pure paths are immutable, hashable, comparable, and support the `/`
operator and `os.fspath()`.

## Concrete paths

Subclasses that add filesystem I/O:

| Class | Description |
|---|---|
| `Path(*pathsegments)` | Concrete path for the platform |
| `PosixPath(*pathsegments)` | Concrete POSIX filesystem paths |
| `WindowsPath(*pathsegments)` | Concrete Windows filesystem paths |

All `PurePath` methods plus:

| Category | Methods |
|---|---|
| Status | `stat()`, `lstat()`, `exists()`, `is_file()`, `is_dir()`, `is_symlink()`, `is_mount()`, `is_junction()`, `is_block_device()`, `is_char_device()`, `is_fifo()`, `is_socket()`, `samefile()`, `owner()`, `group()` |
| Resolution | `resolve()`, `absolute()`, `readlink()`, `expanduser()` |
| I/O | `open()`, `read_bytes()`, `read_text()`, `write_bytes()`, `write_text()` |
| Mutation | `mkdir()`, `rmdir()`, `touch()`, `unlink()`, `chmod()`, `lchmod()`, `rename()`, `replace()`, `symlink_to()`, `hardlink_to()` |
| Traversal | `iterdir()`, `walk()`, `glob()`, `rglob()` |
| Tree ops | `copy()`, `copy_into()`, `move()`, `move_into()`, `delete()` |
| Class methods | `Path.cwd()`, `Path.home()` |

## Benchmarks

*Release mode, macOS 15 (Apple M1), Python 3.14.*

Stat and I/O operations are 1.3–1.4× faster (GIL released during syscalls).
Stem/suffix are 1.5–1.8× faster (pure Rust string ops). Construction and
some globs are slower — a [performance optimization plan](docs/OPTIMIZATION.md)
is in progress.

Full results: [`docs/BENCHMARKS.md`](docs/BENCHMARKS.md)

## Development

```bash
make install            # one-time: uv sync + maturin develop
make test               # run all tests (Rust + Python)
make test-windows       # validate Windows path parsing on Linux/Mac
make check              # format check + lint + tests (run before committing)
make ci                 # full CI pipeline locally
make bench              # run release-mode benchmarks
make help               # see all targets
```

CI uses the same `make` targets — no drift between local and remote.

## More

- [Design document](docs/DESIGN.md) — architecture, decisions, class hierarchy
- [Task tracker](docs/CHECKLIST.md) — feature coverage, completion status
- [Benchmarks](docs/BENCHMARKS.md) — full per-operation results + analysis
- [Optimization plan](docs/OPTIMIZATION.md) — three-wins strategy for perf parity
- [Dev guide](AGENTS.md) — build system, conventions, CI, troubleshooting
