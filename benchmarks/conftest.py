"""Shared fixtures for the pathlibrs benchmark suite."""

import pathlib
from pathlib import Path as PyPath

import pathlibrs
import pytest

# Number of iterations for pure ops (no I/O)
N_PURE = 10_000
# Number of files for iter/glob benchmarks
N_FILES = 1_000
# Directory tree dimensions for walk/rglob
TREE_DEPTH = 4
TREE_WIDTH = 4


def pick_path_cls(impl: str) -> tuple[str, type]:
    """Return (label, Path-like-class) for the parametrized implementation."""
    if impl == "pathlibrs":
        return ("pathlibrs", pathlibrs.Path)
    return ("pathlib", PyPath)


def pick_pure_cls(impl: str) -> tuple[str, type]:
    """Return (label, PurePath-like-class) for the parametrized implementation."""
    if impl == "pathlibrs":
        return ("pathlibrs", pathlibrs.PurePath)
    return ("pathlib", pathlib.PurePath)


# ---------------------------------------------------------------------------
# Pure operation fixtures
# ---------------------------------------------------------------------------


@pytest.fixture(scope="session")
def pure_path_strings() -> list[str]:
    """A broad sample of path strings for pure-path benchmarks."""
    return [
        "",
        "/",
        ".",
        "..",
        "/usr/bin/python3",
        "/home/user/docs/report.pdf",
        "/var/log/syslog",
        "/tmp/foo/bar/baz.tar.gz",
        "relative/path/to/file.txt",
        "../parent/dir",
        "./current/dir",
        "file.txt",
        "file.tar.gz",
        ".hidden",
        ".hidden.tar.gz",
        "/a/b/c/d/e/f/g",
        "/very/deeply/nested/path/that/takes/a/while/to/parse/file.log",
        "no_extension",
        "name with spaces.txt",
        "/path/with/symbols/!@#$%^&*()",
    ] * (N_PURE // 20)


# ---------------------------------------------------------------------------
# Filesystem fixtures
# ---------------------------------------------------------------------------


@pytest.fixture
def temp_dir(tmp_path: PyPath) -> PyPath:
    """Session-scoped temp dir for benchmarks (reused across parametrize)."""
    return tmp_path


@pytest.fixture
def file_tree(temp_dir: PyPath) -> PyPath:
    """Create a directory tree of fixed depth and width.

    Returns the root directory. Tree layout::

        root/
        ├── 0_0/
        │   ├── 0_1/
        │   │   ├── 0_2/
        │   │   │   ├── 0_3/
        │   │   │   │   └── f
        │   │   │   └── f
        │   │   └── f
        │   └── f
        ├── 1_0/
        │   └── ... (same pattern)
        └── f
    """
    root = temp_dir / "tree"
    root.mkdir(exist_ok=True)

    def _populate(parent: PyPath, depth: int) -> None:
        if depth >= TREE_DEPTH:
            (parent / "f").write_text("leaf")
            return
        (parent / "f").touch()
        for i in range(TREE_WIDTH):
            child = parent / f"{i}_{depth}"
            child.mkdir()
            _populate(child, depth + 1)

    _populate(root, 0)
    return root


@pytest.fixture
def flat_dir(temp_dir: PyPath) -> PyPath:
    """Create a directory with N_FILES flat files for iterdir/glob."""
    d = temp_dir / "flat"
    d.mkdir(exist_ok=True)
    for i in range(N_FILES):
        ext = ".py" if i % 2 == 0 else ".txt"
        (d / f"file_{i:06d}{ext}").write_text(f"content {i}")
    return d


@pytest.fixture
def text_file(temp_dir: PyPath) -> PyPath:
    """A single text file of varying sizes for I/O benchmarks."""
    f = temp_dir / "sample.txt"
    f.write_text("Hello, pathlibrs!\n" * 100)
    return f


@pytest.fixture
def binary_file(temp_dir: PyPath) -> PyPath:
    """A single binary file for I/O benchmarks."""
    f = temp_dir / "sample.bin"
    f.write_bytes(b"\x00\x01\x02\x03" * 1000)
    return f


@pytest.fixture
def tree_for_mutations(temp_dir: PyPath) -> PyPath:
    """A small tree for copy/move/delete benchmarks."""
    src = temp_dir / "mut_src"
    src.mkdir()
    (src / "a.txt").write_text("a")
    (src / "sub").mkdir()
    (src / "sub" / "b.txt").write_text("b")
    (src / "sub" / "subsub").mkdir()
    (src / "sub" / "subsub" / "c.txt").write_text("c")
    return src
