"""Benchmarks: glob and rglob pattern matching."""

from pathlib import Path as PyPath

import pytest

from benchmarks.conftest import pick_path_cls


@pytest.mark.benchmark(group="glob")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_glob_shallow(benchmark, impl: str, flat_dir: PyPath) -> None:
    """glob('*.py') on a flat directory."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(flat_dir)

    def run() -> None:
        list(p.glob("*.py"))

    benchmark(run)


@pytest.mark.benchmark(group="glob")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_glob_brace(benchmark, impl: str, flat_dir: PyPath) -> None:
    """glob with brace expansion on a flat directory."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(flat_dir)

    def run() -> None:
        list(p.glob("*.{py,txt}"))

    benchmark(run)


@pytest.mark.benchmark(group="glob")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_rglob_tree(benchmark, impl: str, file_tree: PyPath) -> None:
    """rglob('*.py') on a directory tree."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(file_tree)

    def run() -> None:
        list(p.rglob("*.py"))

    benchmark(run)


@pytest.mark.benchmark(group="glob")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_rglob_all(benchmark, impl: str, file_tree: PyPath) -> None:
    """rglob('*') on a directory tree — returns every path."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(file_tree)

    def run() -> None:
        list(p.rglob("*"))

    benchmark(run)
