"""Windows flavour tests that run on non-Windows platforms.

Catches PureWindowsPath behavioural gaps before they reach CI.
Run locally before pushing: ``uv run python -m pytest tests/test_windows_flavour.py -v``
These are NOT run in CI — CI has actual Windows runners.
"""
import os

import pytest

from pathlibrs import PurePosixPath, PureWindowsPath


# ══════════════════════════════════════════════════════════
# String representation — must use platform-native separators
# ══════════════════════════════════════════════════════════

@pytest.mark.xfail(reason="Windows str/fspath separator not implemented")
def test_str_windows_separator() -> None:
    """str(PureWindowsPath('a/b/c')) uses \\ separators."""
    p = PureWindowsPath("a/b/c")
    assert str(p) == "a\\b\\c", f"got {str(p)!r}"


@pytest.mark.xfail(reason="Windows str/fspath separator not implemented")
def test_fspath_windows_separator() -> None:
    """os.fspath(PureWindowsPath('a/b')) uses \\ separators."""
    p = PureWindowsPath("a/b")
    assert os.fspath(p) == "a\\b", f"got {os.fspath(p)!r}"


def test_str_posix_separator() -> None:
    """str(PurePosixPath('a/b/c')) uses / separators."""
    p = PurePosixPath("a/b/c")
    assert str(p) == "a/b/c"


def test_fspath_posix_separator() -> None:
    """os.fspath(PurePosixPath('a/b')) uses / separators."""
    p = PurePosixPath("a/b")
    assert os.fspath(p) == "a/b"


# ══════════════════════════════════════════════════════════
# Drive parsing
# ══════════════════════════════════════════════════════════

def test_drive_relative_windows() -> None:
    """c:foo/bar is a relative Windows path with drive c:."""
    p = PureWindowsPath("c:a/b")
    assert p.drive == "c:", f"got {p.drive!r}"
    assert p.root == "", f"got {p.root!r}"


def test_drive_absolute_windows() -> None:
    """c:/foo is an absolute Windows path."""
    p = PureWindowsPath("c:/foo")
    assert p.drive == "c:", f"got {p.drive!r}"
    assert p.root == "\\", f"got {p.root!r}"


# ══════════════════════════════════════════════════════════
# Root
# ══════════════════════════════════════════════════════════

def test_root_windows_separator() -> None:
    """PureWindowsPath root uses \\ separator."""
    p = PureWindowsPath("/a/b")
    assert p.root == "\\", f"got {p.root!r}"


def test_root_posix_separator() -> None:
    """PurePosixPath root uses / separator."""
    p = PurePosixPath("/a/b")
    assert p.root == "/", f"got {p.root!r}"


# ══════════════════════════════════════════════════════════
# is_absolute
# ══════════════════════════════════════════════════════════

def test_is_absolute_windows_drive_root() -> None:
    """c:/windows is absolute on Windows."""
    p = PureWindowsPath("c:/windows")
    assert p.is_absolute(), f"got {p.is_absolute()}"


def test_is_absolute_windows_drive_no_root() -> None:
    """c:windows is NOT absolute on Windows (drive but no root)."""
    p = PureWindowsPath("c:windows")
    assert not p.is_absolute(), f"got {p.is_absolute()}"


@pytest.mark.xfail(reason="Windows is_absolute parsing gap")
def test_is_absolute_windows_root_only() -> None:
    """\\foo is NOT absolute on Windows — needs a drive."""
    p = PureWindowsPath("\\foo")
    assert not p.is_absolute(), f"got {p.is_absolute()}"


# ══════════════════════════════════════════════════════════
# Drive case-insensitive matching
# ══════════════════════════════════════════════════════════

@pytest.mark.xfail(reason="case-insensitive drive matching not implemented")
def test_is_relative_to_case_insensitive_drive() -> None:
    """C:Foo/Bar is relative to c: (case-insensitive drive)."""
    p = PureWindowsPath("C:Foo/Bar")
    assert p.is_relative_to("c:"), f"got {p.is_relative_to('c:')}"


@pytest.mark.xfail(reason="case-insensitive drive matching not implemented")
def test_relative_to_case_insensitive_drive() -> None:
    """C:Foo/Bar relative to c: strips the drive."""
    p = PureWindowsPath("C:Foo/Bar")
    result = p.relative_to("c:")
    assert str(result) == "Foo\\Bar", f"got {str(result)!r}"


# ══════════════════════════════════════════════════════════
# Equality
# ══════════════════════════════════════════════════════════

def test_eq_different_drives() -> None:
    """Paths with different drives are not equal."""
    assert PureWindowsPath("c:foo") != PureWindowsPath("d:foo")


def test_eq_drive_relative_vs_absolute() -> None:
    """c:a/b != c:/a/b (relative vs absolute)."""
    assert PureWindowsPath("c:a/b") != PureWindowsPath("c:/a/b")


# ══════════════════════════════════════════════════════════
# Validation
# ══════════════════════════════════════════════════════════

@pytest.mark.xfail(reason="colon validation not implemented for Windows")
def test_with_name_rejects_colon() -> None:
    """with_name(':') raises ValueError — colon is invalid on Windows."""
    p = PureWindowsPath("/dir/file.txt")
    with pytest.raises(ValueError, match="Invalid"):
        p.with_name(":")


@pytest.mark.xfail(reason="colon validation not implemented for Windows")
def test_with_stem_rejects_colon() -> None:
    """with_stem(':') raises ValueError — colon is invalid on Windows."""
    p = PureWindowsPath("/dir/file.txt")
    with pytest.raises(ValueError, match="Invalid"):
        p.with_stem(":")


# ══════════════════════════════════════════════════════════
# _parse_path classmethod
# ══════════════════════════════════════════════════════════

@pytest.mark.xfail(reason="_parse_path not implemented")
def test_parse_path_classmethod() -> None:
    """PureWindowsPath has _parse_path classmethod."""
    assert hasattr(PureWindowsPath, "_parse_path"), "missing _parse_path"
    assert callable(PureWindowsPath._parse_path)


# ══════════════════════════════════════════════════════════
# Nested constructor with mixed flavours
# ══════════════════════════════════════════════════════════

@pytest.mark.xfail(reason="variadic __new__ not implemented")
def test_constructor_nested_foreign_flavour() -> None:
    """PureWindowsPath(PurePosixPath('b'), PureWindowsPath('c:/d')) preserves segments."""
    p = PureWindowsPath(PurePosixPath("b"), PureWindowsPath("c:/d"))
    expected = PureWindowsPath("b", "c:\\d")
    assert p == expected, f"got {str(p)!r}, expected {str(expected)!r}"


# ══════════════════════════════════════════════════════════
# Unicode
# ══════════════════════════════════════════════════════════

def test_unicode_path_windows() -> None:
    """PureWindowsPath handles Unicode characters."""
    p = PureWindowsPath("/home/ユーザー")
    assert "ユーザー" in str(p)
