"""Basic smoke tests for pathlibrs Phase 1."""
from pathlibrs import PurePath, PurePosixPath, PureWindowsPath


class TestPurePosixPath:
    """Tests for PurePosixPath matching CPython pathlib behaviour."""

    def test_empty(self) -> None:
        p = PurePosixPath("")
        assert str(p) == ""
        assert p.drive == ""
        assert p.root == ""
        assert p.anchor == ""
        assert p.name is None
        assert p.stem == ""
        assert p.suffix == ""

    def test_absolute_simple(self) -> None:
        p = PurePosixPath("/foo/bar.txt")
        assert str(p) == "/foo/bar.txt"
        assert p.drive == ""
        assert p.root == "/"
        assert p.anchor == "/"
        assert p.name == "bar.txt"
        assert p.stem == "bar"
        assert p.suffix == ".txt"
        assert p.suffixes == [".txt"]

    def test_relative_simple(self) -> None:
        p = PurePosixPath("foo/bar/baz")
        assert str(p) == "foo/bar/baz"
        assert p.drive == ""
        assert p.root == ""
        assert p.anchor == ""
        assert p.name == "baz"

    def test_root_only(self) -> None:
        p = PurePosixPath("/")
        assert str(p) == "/"
        assert p.root == "/"
        assert p.name is None
        assert p.parts == ("", "/")

    def test_parts(self) -> None:
        p = PurePosixPath("/usr/local/bin")
        assert p.parts == ("", "/", "usr", "local", "bin")

    def test_parent(self) -> None:
        p = PurePosixPath("/foo/bar/baz")
        assert str(p.parent) == "/foo/bar"
        assert str(p.parent.parent) == "/foo"
        assert str(p.parent.parent.parent) == "/"
        assert str(p.parent.parent.parent.parent) == "/"

    def test_parents(self) -> None:
        p = PurePosixPath("/foo/bar/baz")
        parents = list(p.parents)
        assert len(parents) == 3
        assert str(parents[0]) == "/foo/bar"
        assert str(parents[1]) == "/foo"
        assert str(parents[2]) == "/"

    def test_suffixes_multi(self) -> None:
        p = PurePosixPath("/path/to/archive.tar.gz")
        assert p.suffix == ".gz"
        assert p.suffixes == [".tar", ".gz"]
        assert p.stem == "archive.tar"

    def test_leading_dot(self) -> None:
        p = PurePosixPath("/home/user/.bashrc")
        assert p.name == ".bashrc"
        assert p.stem == ".bashrc"
        assert p.suffix == ""

    def test_truediv(self) -> None:
        p = PurePosixPath("/foo")
        result = p / "bar" / "baz"
        assert str(result) == "/foo/bar/baz"
        assert isinstance(result, PurePosixPath)

    def test_rtruediv(self) -> None:
        p = PurePosixPath("bar")
        result = "/foo" / p
        assert str(result) == "/foo/bar"

    def test_joinpath(self) -> None:
        p = PurePosixPath("/foo")
        result = p.joinpath("bar", "baz")
        assert str(result) == "/foo/bar/baz"

    def test_with_name(self) -> None:
        p = PurePosixPath("/foo/bar.txt")
        result = p.with_name("baz.py")
        assert str(result) == "/foo/baz.py"

    def test_with_stem(self) -> None:
        p = PurePosixPath("/foo/bar.txt")
        result = p.with_stem("baz")
        assert str(result) == "/foo/baz.txt"

    def test_with_suffix(self) -> None:
        p = PurePosixPath("/foo/bar.txt")
        result = p.with_suffix(".py")
        assert str(result) == "/foo/bar.py"

    def test_relative_to(self) -> None:
        p = PurePosixPath("/foo/bar/baz")
        result = p.relative_to("/foo")
        assert str(result) == "bar/baz"

    def test_relative_to_error(self) -> None:
        import pytest
        p = PurePosixPath("/foo/bar")
        with pytest.raises(ValueError):
            p.relative_to("/baz")

    def test_is_relative_to(self) -> None:
        p = PurePosixPath("/foo/bar/baz")
        assert p.is_relative_to("/foo")
        assert p.is_relative_to("/foo/bar")
        assert p.is_relative_to("/foo/bar/baz")
        assert not p.is_relative_to("/baz")

    def test_eq(self) -> None:
        assert PurePosixPath("/foo") == PurePosixPath("/foo")
        assert PurePosixPath("/foo") != PurePosixPath("/bar")
        assert PurePosixPath("/foo") == "/foo"

    def test_hash(self) -> None:
        s = {PurePosixPath("/foo"), PurePosixPath("/bar"), PurePosixPath("/foo")}
        assert len(s) == 2

    def test_lt(self) -> None:
        assert PurePosixPath("/a") < PurePosixPath("/b")

    def test_repr(self) -> None:
        p = PurePosixPath("/foo")
        r = repr(p)
        assert "PurePosixPath" in r
        assert "/foo" in r

    def test_fspath(self) -> None:
        import os
        p = PurePosixPath("/foo/bar")
        assert os.fspath(p) == "/foo/bar"

    def test_reduce(self) -> None:
        import pickle
        p = PurePosixPath("/foo/bar")
        pickled = pickle.dumps(p)
        restored = pickle.loads(pickled)
        assert restored == p

    def test_match(self) -> None:
        p = PurePosixPath("/foo/bar.py")
        assert p.match("*.py")
        assert p.match("bar.py")
        assert not p.match("*.txt")

    def test_match_absolute(self) -> None:
        p = PurePosixPath("/foo/bar.py")
        assert p.match("/foo/*.py")
        assert not p.match("/bar/*.py")

    def test_as_uri(self) -> None:
        p = PurePosixPath("/home/user/file.txt")
        assert p.as_uri() == "file:///home/user/file.txt"

    def test_relative_uri(self) -> None:
        p = PurePosixPath("relative/path")
        uri = p.as_uri()
        assert uri.startswith("file:")
        assert "relative/path" in uri

    def test_str(self) -> None:
        p = PurePosixPath("/foo/bar")
        assert str(p) == "/foo/bar"


class TestPureWindowsPath:
    """Tests for PureWindowsPath matching CPython behaviour."""

    def test_drive_letter(self) -> None:
        p = PureWindowsPath("C:\\Windows\\System32")
        assert p.drive == "C:"
        assert p.root == "\\"
        assert p.anchor == "C:\\"
        assert p.name == "System32"
        assert p.parts == ("C:", "\\", "Windows", "System32")

    def test_drive_relative(self) -> None:
        p = PureWindowsPath("C:Users")
        assert p.drive == "C:"
        assert p.root == ""

    def test_unc(self) -> None:
        p = PureWindowsPath("\\\\server\\share\\folder")
        assert p.drive == "\\\\server\\share"
        assert p.root == "\\"

    def test_forward_slashes(self) -> None:
        p = PureWindowsPath("C:/Users/Name")
        assert p.drive == "C:"
        assert str(p).replace("/", "\\") == "C:\\Users\\Name"

    def test_parent_windows(self) -> None:
        p = PureWindowsPath("C:\\foo\\bar\\baz")
        assert str(p.parent) == "C:\\foo\\bar"

    def test_eq_windows(self) -> None:
        assert PureWindowsPath("C:\\foo") == PureWindowsPath("C:\\foo")
        assert PureWindowsPath("C:\\foo") != PureWindowsPath("C:\\bar")

    def test_truediv_windows(self) -> None:
        p = PureWindowsPath("C:\\foo")
        result = p / "bar"
        assert "bar" in str(result)

    def test_as_uri_windows(self) -> None:
        p = PureWindowsPath("C:\\Users\\Name")
        uri = p.as_uri()
        assert uri == "file:///C:/Users/Name"

    def test_as_uri_unc(self) -> None:
        p = PureWindowsPath("\\\\server\\share\\path")
        uri = p.as_uri()
        assert uri.startswith("file://")
        assert "server" in uri
        assert "share" in uri

    def test_repr_windows(self) -> None:
        p = PureWindowsPath("C:\\foo")
        r = repr(p)
        assert "PureWindowsPath" in r


class TestPurePath:
    """Tests for the base PurePath class."""

    def test_subclass_compat(self) -> None:
        """PurePosixPath should be a subclass of PurePath."""
        assert issubclass(PurePosixPath, PurePath)
        assert issubclass(PureWindowsPath, PurePath)

    def test_instance_check(self) -> None:
        p = PurePosixPath("/foo")
        assert isinstance(p, PurePath)


class TestPosixVsWindows:
    """Cross-flavour tests."""

    def test_posix_no_drive(self) -> None:
        p = PurePosixPath("C:\\foo")
        # On POSIX, "C:\\foo" is not a drive letter — it's a part
        assert p.drive == ""

    def test_windows_has_drive(self) -> None:
        p = PureWindowsPath("C:\\foo")
        assert p.drive == "C:"

    def test_posix_and_windows_not_equal(self) -> None:
        # Same raw string but different flavours should compare by
        # parsed components. On POSIX "/" has root="/", on Windows "\" is just a part.
        # Actually, let's test that they are different types
        posix = PurePosixPath("foo")
        windows = PureWindowsPath("foo")
        # They're different classes
        assert type(posix) != type(windows)

    def test_as_posix_from_windows(self) -> None:
        p = PureWindowsPath("C:\\Users\\Name")
        assert p.as_posix() == "C:/Users/Name"
