"""Benchmarks: filesystem mutation operations."""

from pathlib import Path as PyPath

import pytest

from benchmarks.conftest import pick_path_cls


@pytest.mark.benchmark(group="mutation")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_mkdir(benchmark, impl: str, temp_dir: PyPath) -> None:
    """mkdir() single-level directory."""
    _label, path_cls = pick_path_cls(impl)
    base = path_cls(temp_dir / "mkdir_bench")
    base.mkdir(exist_ok=True)
    idx = [0]

    def run() -> None:
        i = idx[0]
        idx[0] += 1
        p = base / str(i)
        p.mkdir()

    benchmark(run)


@pytest.mark.benchmark(group="mutation")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_mkdir_parents(benchmark, impl: str, temp_dir: PyPath) -> None:
    """mkdir(parents=True) creating a 3-level tree."""
    _label, path_cls = pick_path_cls(impl)
    base = path_cls(temp_dir / "mkdirp_bench")
    idx = [0]

    def run() -> None:
        i = idx[0]
        idx[0] += 1
        p = base / str(i) / "a" / "b" / "c"
        p.mkdir(parents=True)

    benchmark(run)


@pytest.mark.benchmark(group="mutation")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_touch_unlink(benchmark, impl: str, temp_dir: PyPath) -> None:
    """touch() + unlink() cycle."""
    _label, path_cls = pick_path_cls(impl)
    base = path_cls(temp_dir / "touch_unlink_bench")
    base.mkdir(exist_ok=True)
    idx = [0]

    def run() -> None:
        i = idx[0]
        idx[0] += 1
        p = base / str(i)
        p.touch()
        p.unlink()

    benchmark(run)


@pytest.mark.benchmark(group="mutation")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_copy_tree(benchmark, impl: str, temp_dir: PyPath, tree_for_mutations: PyPath) -> None:
    """copy() a small directory tree."""
    _label, path_cls = pick_path_cls(impl)
    src = path_cls(tree_for_mutations)
    dst_base = path_cls(temp_dir / "copy_bench")
    dst_base.mkdir(exist_ok=True)
    idx = [0]

    def run() -> None:
        i = idx[0]
        idx[0] += 1
        dst = dst_base / f"dst_{i}"
        src.copy(dst)

    benchmark(run)


@pytest.mark.benchmark(group="mutation")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_move_tree(benchmark, impl: str, temp_dir: PyPath) -> None:
    """move() a small directory tree."""
    _label, path_cls = pick_path_cls(impl)
    base_src = path_cls(temp_dir / "move_src")
    base_dst = path_cls(temp_dir / "move_dst")
    base_dst.mkdir(exist_ok=True)
    idx = [0]

    def run() -> None:
        i = idx[0]
        idx[0] += 1
        src = base_src / str(i)
        src.mkdir(parents=True)
        (src / "a.txt").write_text("a")
        dst = base_dst / str(i)
        src.move(dst)

    benchmark(run)


@pytest.mark.benchmark(group="mutation")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_delete_tree(benchmark, impl: str, temp_dir: PyPath) -> None:
    """delete() a small directory tree."""
    _label, path_cls = pick_path_cls(impl)
    base_del = path_cls(temp_dir / "delete_src")
    base_del.mkdir(exist_ok=True)

    if not hasattr(path_cls, "delete"):
        pytest.skip(f"{path_cls.__name__}.delete() not available on this Python version")

    idx = [0]

    def run() -> None:
        i = idx[0]
        idx[0] += 1
        src = base_del / str(i)
        src.mkdir()
        (src / "x.txt").write_text("x")
        (src / "sub").mkdir()
        (src / "sub" / "y.txt").write_text("y")
        src.delete()

    benchmark(run)
