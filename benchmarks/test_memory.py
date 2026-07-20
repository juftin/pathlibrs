"""Benchmarks: memory usage and object size."""

import sys
import tracemalloc

import pytest

from benchmarks.conftest import N_PURE, pick_pure_cls


@pytest.mark.benchmark(group="memory")
def test_object_size_pathlibrs(benchmark) -> None:
    """sys.getsizeof() for 100k pathlibrs.PurePath instances."""
    import pathlibrs

    def run() -> None:
        for _ in range(N_PURE):
            p = pathlibrs.PurePath("/usr/bin/python3")
            sys.getsizeof(p)

    benchmark(run)


@pytest.mark.benchmark(group="memory")
def test_object_size_pathlib(benchmark) -> None:
    """sys.getsizeof() for 100k pathlib.PurePath instances."""
    import pathlib

    def run() -> None:
        for _ in range(N_PURE):
            p = pathlib.PurePath("/usr/bin/python3")
            sys.getsizeof(p)

    benchmark(run)


@pytest.mark.benchmark(group="memory")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_construct_and_discard(benchmark, impl: str) -> None:
    """Construct and discard 10k PurePath objects — measures alloc pressure."""
    _label, pure_cls = pick_pure_cls(impl)

    def run() -> None:
        for _ in range(N_PURE):
            _p = pure_cls("/a/b/c/d/e/f/g/file.txt")

    benchmark(run)


@pytest.mark.benchmark(group="memory")
def test_tracemalloc_construct_pathlibrs(benchmark) -> None:
    """tracemalloc snapshot for constructing 10k pathlibrs PurePath objects."""
    import pathlibrs

    def run() -> None:
        tracemalloc.start()
        for _ in range(N_PURE):
            _p = pathlibrs.PurePath("/usr/bin/python3")
        _snapshot = tracemalloc.take_snapshot()
        tracemalloc.stop()

    benchmark(run)


@pytest.mark.benchmark(group="memory")
def test_tracemalloc_construct_pathlib(benchmark) -> None:
    """tracemalloc snapshot for constructing 10k pathlib PurePath objects."""
    import pathlib

    def run() -> None:
        tracemalloc.start()
        for _ in range(N_PURE):
            _p = pathlib.PurePath("/usr/bin/python3")
        _snapshot = tracemalloc.take_snapshot()
        tracemalloc.stop()

    benchmark(run)
