# pathlibrs

A fast pure-Rust implementation of Python's pathlib, with drop-in replacement classes.

## Phase 1 — Pure Paths

Pure (non-IO) path classes matching CPython 3.12 pathlib:

- `PurePath` — base class
- `PurePosixPath` — POSIX-style paths (`/` separator, no drive letters)
- `PureWindowsPath` — Windows-style paths (drive letters, UNC, both `\` and `/`)

## Installation

```bash
pip install pathlibrs
```

Or from source:

```bash
maturin develop
```

## Quick Start

```python
from pathlibrs import PurePosixPath, PureWindowsPath

# POSIX
p = PurePosixPath("/usr/local/bin/python3")
print(p.parts)    # ('', '/', 'usr', 'local', 'bin', 'python3')
print(p.parent)   # /usr/local/bin
print(p.name)     # python3
print(p.stem)     # python
print(p.suffix)   # 3

# Windows
p = PureWindowsPath("C:\\Users\\Name\\Documents")
print(p.drive)    # C:
print(p.root)     # \
print(p.parts)    # ('C:', '\\', 'Users', 'Name', 'Documents')

# Join paths
base = PurePosixPath("/home/user")
full = base / "projects" / "pathlibrs"

# Pattern matching
p = PurePosixPath("/path/to/file.py")
p.match("*.py")   # True
```

## Development

```bash
# Build and install for development
maturin develop

# Run tests
python -m pytest tests/

# Build release wheel
maturin build --release
```
