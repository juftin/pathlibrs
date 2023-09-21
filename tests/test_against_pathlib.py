"""
Testing PathRS against Path(
"""

from pathlib import Path

from pathlibrs import Path as PathRS


def test_generic_file(pathlib_test_file: Path, rs_test_file: PathRS) -> None:
    """
    Test on a generic file name
    """
    assert str(pathlib_test_file) == str(rs_test_file)


def test_generic_dir() -> None:
    """
    Test on a generic directory name
    """
    p = Path("test")
    r = PathRS("test")
    assert str(p) == str(r)


def test_file_with_name(pathlib_test_file: Path, rs_test_file: PathRS) -> None:
    """
    Test on a file with a name
    """
    p = pathlib_test_file.with_name("test2.txt")
    r = rs_test_file.with_name("test2.txt")
    assert str(p) == str(r)


def test_file_with_stem(pathlib_test_file: Path, rs_test_file: PathRS) -> None:
    """
    Test on a file rename with a stem
    """
    p = pathlib_test_file.with_stem("test2")
    r = rs_test_file.with_stem("test2")
    assert str(p) == str(r)


def test_file_with_suffix(pathlib_test_file: Path, rs_test_file: PathRS) -> None:
    """
    Test on a file rename with a suffix
    """
    p = pathlib_test_file.with_suffix(".rst")
    r = rs_test_file.with_suffix(".rst")
    assert str(p) == str(r)


def test_read_text(readme_path: Path) -> None:
    """
    Test reading text from a file
    """
    p = readme_path.read_text()
    r = PathRS(str(readme_path)).read_text()
    assert p == r


# def test_write_text(tmp_path: Path) -> None:
#     """
#     Test writing text to a file
#     """
#     r_name = "test_pathrs.txt"
#     p = tmp_path / "test_pathlib.txt"
#     r = PathRS(str(p)) / r_name
#     test_text = "This is a test of file writing\nYay!"
#     p.write_text(test_text)
#     r.write_text(test_text)
#     p_text = p.read_text()
#     r_text = p.with_name(r_name).read_text()
#     assert p_text == r_text
