"""Benchmarks: directory listing and traversal."""

from pathlib import Path as PyPath

import pytest

from benchmarks.conftest import pick_path_cls


@pytest.mark.benchmark(group="dir")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_iterdir_flat(benchmark, impl: str, flat_dir: PyPath) -> None:
    """iterdir() on a flat directory with N_FILES entries."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(flat_dir)

    def run() -> None:
        list(p.iterdir())

    benchmark(run)


@pytest.mark.benchmark(group="dir")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_walk_tree(benchmark, impl: str, file_tree: PyPath) -> None:
    """walk() on a tree of depth 4, width 4."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(file_tree)

    def run() -> None:
        count = 0
        for _root, _dirs, _files in p.walk():
            count += len(_files)

    benchmark(run)
