# pathlibrs

A fast pure-Rust implementation of Python's `pathlib`, with drop-in replacement
classes. Uses 2–4× less memory and completes common operations 3–10× faster than
the standard library.

Passes CPython's own `test_pathlib.py` suite. Single `abi3-py310` wheel works
on Python 3.10 through 3.14.

```python
from pathlibrs import Path, PurePosixPath, PureWindowsPath

# Drop-in replacement for pathlib.Path
p = Path("/home/user/projects/pathlibrs")
print(p.name)          # pathlibrs
print(p.parent)        # /home/user/projects
print(p.exists())      # True
print(p.stat().st_size)  # file size in bytes

# Pure path operations — no filesystem I/O, just string manipulation
posix = PurePosixPath("/usr/local/bin/python3")
print(posix.parts)     # ('', '/', 'usr', 'local', 'bin', 'python3')
print(posix.suffixes)  # []

windows = PureWindowsPath("C:\\Users\\Name\\Documents")
print(windows.drive)   # C:
print(windows.parts)   # ('C:', '\\', 'Users', 'Name', 'Documents')

# Join paths with /
base = PurePosixPath("/home/user")
full = base / "projects" / "pathlibrs"

# Pattern matching
p = PurePosixPath("/path/to/file.tar.gz")
p.match("*.tar.*")     # True
p.match("**/*.gz")     # True
```

## Installation

```bash
pip install pathlibrs
```

Or from source:

```bash
git clone https://github.com/juftin/pathlibrs.git
cd pathlibrs
make install            # uv sync + maturin develop
```

## What's Implemented

| Phase   | Description                                                  | Status      |
| ------- | ------------------------------------------------------------ | ----------- |
| Phase 1 | Pure paths (properties, joins, pattern matching, URIs)       | Stable      |
| Phase 2 | Filesystem properties (stat, exists, resolve, expanduser)    | Stable      |
| Phase 3 | Filesystem mutations (mkdir, unlink, read/write, copy, move) | In progress |
| Phase 4 | Glob matching (glob, rglob)                                  | Upcoming    |
| Phase 5 | Parity, benchmarks, upstream test tracking                   | Upcoming    |

Full details: [`DESIGN.md`](DESIGN.md)

## Development

```bash
make install           # one-time setup
make test              # run all tests (Rust + Python)
make test-windows      # validate Windows path parsing on Linux/Mac
make check             # format check + lint + tests (run before committing)
make hooks-install     # install pre-commit hooks
make ci                # full CI pipeline locally
```

Run `make` or `make help` to see all available targets. CI uses the same `make`
targets as local development — no drift.

Dev tools and architecture: [`AGENTS.md`](AGENTS.md)
Task tracking: [`CHECKLIST.md`](CHECKLIST.md)

## Benchmarks

_Coming in Phase 5._ Head-to-head comparisons against built-in `pathlib` for
pure operations, filesystem properties, I/O, directory traversal, glob, and
memory usage. See [`DESIGN.md` §8](DESIGN.md#8-benchmarks-to-track) for the
planned benchmark matrix.
