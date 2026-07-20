"""Benchmarks: pure path operations — no filesystem I/O."""

import pytest

from benchmarks.conftest import N_PURE, pick_pure_cls


# Paths that have a name component (exclude "", "/", ".", "..")
def _named_paths(strings: list[str]) -> list[str]:
    """Return only paths that have a non-empty name component."""
    result = []
    import pathlib

    for s in strings:
        try:
            pathlib.PurePath(s).with_name("test")
        except ValueError:
            continue
        result.append(s)
    return result


@pytest.mark.benchmark(group="pure-construct")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_construct_path(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Construct PurePath from strings."""
    _label, pure_cls = pick_pure_cls(impl)
    strings = pure_path_strings

    def run() -> None:
        for s in strings:
            pure_cls(s)

    benchmark(run)


@pytest.mark.benchmark(group="pure-construct")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_construct_from_parts(benchmark, impl: str) -> None:
    """Construct PurePath from multiple string segments."""
    _label, pure_cls = pick_pure_cls(impl)

    def run() -> None:
        for _ in range(N_PURE):
            pure_cls("usr", "bin", "python3")

    benchmark(run)


@pytest.mark.benchmark(group="pure-property")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_parent(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Access .parent on many paths."""
    _label, pure_cls = pick_pure_cls(impl)
    paths = [pure_cls(s) for s in pure_path_strings]

    def run() -> None:
        for p in paths:
            _ = p.parent

    benchmark(run)


@pytest.mark.benchmark(group="pure-property")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_name(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Access .name on many paths."""
    _label, pure_cls = pick_pure_cls(impl)
    paths = [pure_cls(s) for s in pure_path_strings]

    def run() -> None:
        for p in paths:
            _ = p.name

    benchmark(run)


@pytest.mark.benchmark(group="pure-property")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_stem(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Access .stem on many paths."""
    _label, pure_cls = pick_pure_cls(impl)
    paths = [pure_cls(s) for s in pure_path_strings]

    def run() -> None:
        for p in paths:
            _ = p.stem

    benchmark(run)


@pytest.mark.benchmark(group="pure-property")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_suffix(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Access .suffix on many paths."""
    _label, pure_cls = pick_pure_cls(impl)
    paths = [pure_cls(s) for s in pure_path_strings]

    def run() -> None:
        for p in paths:
            _ = p.suffix

    benchmark(run)


@pytest.mark.benchmark(group="pure-property")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_suffixes(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Access .suffixes on many paths."""
    _label, pure_cls = pick_pure_cls(impl)
    paths = [pure_cls(s) for s in pure_path_strings]

    def run() -> None:
        for p in paths:
            _ = p.suffixes

    benchmark(run)


@pytest.mark.benchmark(group="pure-property")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_parts(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Access .parts on many paths."""
    _label, pure_cls = pick_pure_cls(impl)
    paths = [pure_cls(s) for s in pure_path_strings]

    def run() -> None:
        for p in paths:
            _ = p.parts

    benchmark(run)


@pytest.mark.benchmark(group="pure-mutate")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_with_name(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Call .with_name() on many paths."""
    _label, pure_cls = pick_pure_cls(impl)
    strings = _named_paths(pure_path_strings)
    paths = [pure_cls(s) for s in strings]

    def run() -> None:
        for p in paths:
            _ = p.with_name("newname.txt")

    benchmark(run)


@pytest.mark.benchmark(group="pure-mutate")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_with_suffix(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Call .with_suffix() on many paths."""
    _label, pure_cls = pick_pure_cls(impl)
    strings = _named_paths(pure_path_strings)
    paths = [pure_cls(s) for s in strings]

    def run() -> None:
        for p in paths:
            _ = p.with_suffix(".log")

    benchmark(run)


@pytest.mark.benchmark(group="pure-mutate")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_with_stem(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Call .with_stem() on many paths."""
    _label, pure_cls = pick_pure_cls(impl)
    strings = _named_paths(pure_path_strings)
    paths = [pure_cls(s) for s in strings]

    def run() -> None:
        for p in paths:
            _ = p.with_stem("newstem")

    benchmark(run)


@pytest.mark.benchmark(group="pure-mutate")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_joinpath(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Call .joinpath() on many paths."""
    _label, pure_cls = pick_pure_cls(impl)
    paths = [pure_cls(s) for s in pure_path_strings]

    def run() -> None:
        for p in paths:
            _ = p.joinpath("sub", "child")

    benchmark(run)


@pytest.mark.benchmark(group="pure-mutate")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_truediv(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Use / operator on many paths."""
    _label, pure_cls = pick_pure_cls(impl)
    paths = [pure_cls(s) for s in pure_path_strings]

    def run() -> None:
        for p in paths:
            _ = p / "child"

    benchmark(run)


@pytest.mark.benchmark(group="pure-stringify")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_str(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Call str() on many paths."""
    _label, pure_cls = pick_pure_cls(impl)
    paths = [pure_cls(s) for s in pure_path_strings]

    def run() -> None:
        for p in paths:
            _ = str(p)

    benchmark(run)


@pytest.mark.benchmark(group="pure-stringify")
@pytest.mark.parametrize("impl", ["pathlib", "pathlibrs"])
def test_fspath(benchmark, impl: str, pure_path_strings: list[str]) -> None:
    """Call os.fspath() on many paths."""
    import os

    _label, pure_cls = pick_pure_cls(impl)
    paths = [pure_cls(s) for s in pure_path_strings]

    def run() -> None:
        for p in paths:
            _ = os.fspath(p)

    benchmark(run)
