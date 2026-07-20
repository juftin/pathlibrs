"""Benchmarks: filesystem I/O operations."""

from pathlib import Path as PyPath

import pytest

from benchmarks.conftest import pick_path_cls


@pytest.mark.benchmark(group="io")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_read_text(benchmark, impl: str, text_file: PyPath) -> None:
    """read_text() on a small text file (~1.9 KB)."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(text_file)

    def run() -> None:
        for _ in range(500):
            p.read_text()

    benchmark(run)


@pytest.mark.benchmark(group="io")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_read_bytes(benchmark, impl: str, binary_file: PyPath) -> None:
    """read_bytes() on a small binary file (~4 KB)."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(binary_file)

    def run() -> None:
        for _ in range(500):
            p.read_bytes()

    benchmark(run)


@pytest.mark.benchmark(group="io")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_write_text(benchmark, impl: str, temp_dir: PyPath) -> None:
    """write_text() overwriting a file."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(temp_dir / "write_text_bench.txt")
    content = "line\n" * 100

    def run() -> None:
        for _ in range(500):
            p.write_text(content)

    benchmark(run)


@pytest.mark.benchmark(group="io")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_write_bytes(benchmark, impl: str, temp_dir: PyPath) -> None:
    """write_bytes() overwriting a file."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(temp_dir / "write_bytes_bench.bin")
    data = b"\x00\x01\x02\x03" * 1000

    def run() -> None:
        for _ in range(500):
            p.write_bytes(data)

    benchmark(run)


@pytest.mark.benchmark(group="io")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_open_read(benchmark, impl: str, text_file: PyPath) -> None:
    """open() for reading."""
    _label, path_cls = pick_path_cls(impl)
    p = path_cls(text_file)

    def run() -> None:
        for _ in range(500):
            with p.open("r") as f:
                _content = f.read()

    benchmark(run)
