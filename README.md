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

### Testing Windows flavour on non-Windows hosts

The vendored CPython test suite (``tests/vendored/test_pathlib.py``) has
``@needs_windows`` tests that normally **skip** on Linux/macOS because
``PurePath.parser`` is ``posixpath`` (POSIX) rather than ``ntpath`` (Windows).

Pass ``--windows-flavour`` to re-alias ``PurePath`` → ``PureWindowsPath`` at the
module level, which causes those tests to run against Windows-flavour paths on
any host OS:

```bash
python -m pytest tests/ --windows-flavour -v
```

This uses the same vendored CPython tests that run against actual Windows
runners in CI — no separate test file needed.

#### How it works

1. **Module redirect.** ``tests/conftest.py`` monkeypatches
   ``sys.modules["pathlib"] = pathlibrs`` so the vendored tests (which import
   ``pathlib``) run against our implementation.

2. **Flavour alias.** When ``--windows-flavour`` is passed,
   ``pytest_configure`` sets ``pathlibrs.PurePath = pathlibrs.PureWindowsPath``.
   This happens *before* test collection, so every test class that references
   ``pathlib.PurePath`` gets ``PureWindowsPath`` instead.

3. **Skip logic.** The vendored ``setUp`` checks
   ``if self.cls.parser is posixpath: self.skipTest(...)``.  With the alias
   active, ``PurePath.parser`` is ``ntpath`` (not ``posixpath``), so
   ``@needs_windows`` tests **run** and ``@needs_posix`` tests **skip** —
   the correct inversion for Windows-flavour validation.

4. **Two tests are skipped** regardless: ``test_concrete_class`` and
   ``test_concrete_parser``.  These assert ``type(p) is PurePosixPath`` on
   POSIX hosts, which is false when the alias is active.  The collection
   hook marks them ``skip`` automatically.

5. **CI still runs real Windows.**  The flag is for local development only.
   CI uses actual ``windows-latest`` runners, which compile the crate with
   ``#[cfg(windows)]`` active, picking up the native Windows path
   implementation directly — no alias needed.
