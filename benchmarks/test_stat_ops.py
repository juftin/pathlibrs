"""Benchmarks: filesystem stat and metadata operations."""

from pathlib import Path as PyPath

import pytest

from benchmarks.conftest import pick_path_cls


@pytest.mark.benchmark(group="stat")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_exists_hot(benchmark, impl: str, temp_dir: PyPath) -> None:
    """exists() with warm OS cache."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(temp_dir)
    p.exists()  # warm

    def run() -> None:
        for _ in range(10_000):
            p.exists()

    benchmark(run)


@pytest.mark.benchmark(group="stat")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_exists_missing(benchmark, impl: str, temp_dir: PyPath) -> None:
    """exists() on a path that does not exist."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(temp_dir / "nonexistent")

    def run() -> None:
        for _ in range(10_000):
            p.exists()

    benchmark(run)


@pytest.mark.benchmark(group="stat")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_is_file_hot(benchmark, impl: str, temp_dir: PyPath) -> None:
    """is_file() with warm OS cache."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(temp_dir / "testfile.txt")
    p.write_text("hello")
    p.is_file()  # warm

    def run() -> None:
        for _ in range(10_000):
            p.is_file()

    benchmark(run)


@pytest.mark.benchmark(group="stat")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_is_dir_hot(benchmark, impl: str, temp_dir: PyPath) -> None:
    """is_dir() with warm OS cache."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(temp_dir / "testdir")
    p.mkdir()
    p.is_dir()  # warm

    def run() -> None:
        for _ in range(10_000):
            p.is_dir()

    benchmark(run)


@pytest.mark.benchmark(group="stat")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_is_symlink(benchmark, impl: str, temp_dir: PyPath) -> None:
    """is_symlink() on a non-symlink file."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(temp_dir / "regular_file.txt")
    p.write_text("data")

    def run() -> None:
        for _ in range(10_000):
            p.is_symlink()

    benchmark(run)


@pytest.mark.benchmark(group="stat")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_stat(benchmark, impl: str, temp_dir: PyPath) -> None:
    """stat() with warm OS cache."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(temp_dir / "statme.txt")
    p.write_text("data")
    p.stat()  # warm

    def run() -> None:
        for _ in range(1_000):
            p.stat()

    benchmark(run)


@pytest.mark.benchmark(group="stat")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_samefile(benchmark, impl: str, temp_dir: PyPath) -> None:
    """samefile() on the same file."""
    _label, path_cls = pick_path_cls(impl)
    a = path_cls(temp_dir / "samefile_test")
    a.write_text("x")
    b = path_cls(temp_dir / "samefile_test")

    def run() -> None:
        for _ in range(1_000):
            a.samefile(b)

    benchmark(run)
