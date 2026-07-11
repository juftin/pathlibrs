"""Pytest configuration for pathlibrs vendored CPython test suite.

Redirects ``import pathlib`` → ``import pathlibrs as pathlib`` so the
vendored CPython 3.14 test suite runs against our implementation.

The vendored ``tests/vendored/`` directory contains the full CPython 3.14.6
``Lib/test/test_pathlib/`` package, verbatim.  Only ``test_pathlib.py`` is
active in CI; the modular ABC tests (``test_join.py``, ``test_read.py``,
etc.) are vendored for reference but exercise ``pathlib.types`` internals
that pathlibrs does not implement.

``--windows-flavour`` flag
    When passed, ``PurePath`` is aliased to ``PureWindowsPath`` so the
    vendored ``@needs_windows`` tests run on any host OS.  Use this to
    validate Windows-flavour behaviour without a Windows CI runner::

        uv run python -m pytest tests/ --windows-flavour -v
"""

import os
import sys

# ── Redirect pathlib → pathlibrs ────────────────────────────────────────────
import pathlibrs
import pytest

sys.modules["pathlib"] = pathlibrs

# ── Register pathlib._local for Python 3.13 pickle compatibility ───────────
# CPython's Lib/pathlib/_local.py exists so pathlib objects pickled under
# Python 3.13 (which reference ``pathlib._local``) can be unpickled in 3.14+.
# It is just ``from pathlib import *``.  pathlibrs doesn't ship a ``_local``
# submodule, so we inject one dynamically when the vendored test runs.
import types  # noqa: E402

_local = types.ModuleType("pathlib._local")
_local.__doc__ = "Shim for Python 3.13 pickle compatibility (injected by pathlibrs test harness)."
# Re-export everything from pathlib (which is actually pathlibrs) — same as CPython.
for _attr in dir(pathlibrs):
    if not _attr.startswith("_"):
        setattr(_local, _attr, getattr(pathlibrs, _attr))
sys.modules["pathlib._local"] = _local

# ── Exclude modular ABC tests from discovery ────────────────────────────────
# These test files import from CPython-private ``pathlib.types`` / ``pathlib._os``
# which pathlibrs does not implement per DESIGN.md §11.5.
collect_ignore = [
    os.path.join(os.path.dirname(__file__), "vendored", "test_join.py"),
    os.path.join(os.path.dirname(__file__), "vendored", "test_join_posix.py"),
    os.path.join(os.path.dirname(__file__), "vendored", "test_join_windows.py"),
    os.path.join(os.path.dirname(__file__), "vendored", "test_copy.py"),
    os.path.join(os.path.dirname(__file__), "vendored", "test_read.py"),
    os.path.join(os.path.dirname(__file__), "vendored", "test_write.py"),
]

# ── Patch test.support with missing symbols from CPython 3.14 ─────────────────
# The vendored CPython 3.14 test suite imports symbols (e.g. is_wasm32)
# that only exist in Python 3.14+. Patch them onto the real module.
# These imports are optional — the conftest still loads without them (e.g.
# for ``--windows-flavour`` on non-CPython or uv-managed Pythons that lack
# the ``test`` package).

_TEST_SUPPORT_AVAILABLE = False
try:
    import test.support  # noqa: TC002

    _TEST_SUPPORT_AVAILABLE = True
    if not hasattr(test.support, "is_wasm32"):
        test.support.is_wasm32 = False
    if not hasattr(test.support, "is_emscripten"):
        test.support.is_emscripten = False
    if not hasattr(test.support, "is_wasi"):
        test.support.is_wasi = False

    import test.support.os_helper as os_helper

    if not hasattr(os_helper, "skip_unless_working_chmod"):
        os_helper.skip_unless_working_chmod = lambda fn: fn
    if not hasattr(os_helper, "skip_unless_hardlink"):
        os_helper.skip_unless_hardlink = lambda fn: fn
    if not hasattr(os_helper, "skip_if_dac_override"):
        os_helper.skip_if_dac_override = lambda fn: fn

    import test.support.import_helper as import_helper

    if not hasattr(import_helper, "ensure_lazy_imports"):

        def _ensure_lazy_imports(module_name, lazy_imports):
            """No-op shim for CPython 3.14's ensure_lazy_imports."""

        import_helper.ensure_lazy_imports = _ensure_lazy_imports
except ImportError:
    pass


# ── Skip tests listed in skips.txt ───────────────────────────────────────────


def _load_skips():
    """Load test skip patterns from skips.txt.

    Returns
    -------
    tuple[set[tuple[str, str]], set[str]]
        (method_skips, class_skips)

        * ``method_skips`` — ``{(ClassName, method_name), ...}`` for individual methods.
        * ``class_skips`` — ``{ClassName, ...}`` for entire classes (``ClassName.*``).
    """
    skips_file = os.path.join(os.path.dirname(__file__), "skips.txt")
    method_skips: set[tuple[str, str]] = set()
    class_skips: set[str] = set()
    if not os.path.exists(skips_file):
        return method_skips, class_skips

    with open(skips_file) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split(None, 1)
            if not parts:
                continue
            class_method = parts[0]
            if "." in class_method:
                cls_name, method = class_method.split(".", 1)
                if method == "*":
                    class_skips.add(cls_name)
                else:
                    method_skips.add((cls_name, method))
    return method_skips, class_skips


_METHOD_SKIPS, _CLASS_SKIPS = _load_skips()


def pytest_addoption(parser):
    """Register --windows-flavour flag."""
    parser.addoption(
        "--windows-flavour",
        action="store_true",
        default=False,
        help="Run @needs_windows tests on non-Windows platforms",
    )


def pytest_configure(config):
    """Register custom markers and apply --windows-flavour if requested."""
    config.addinivalue_line(
        "markers", "skip_vendored: skip a vendored CPython test (from skips.txt)"
    )

    # When --windows-flavour is set, re-alias PurePath → PureWindowsPath
    # so @needs_windows tests run on any host OS.
    if config.getoption("--windows-flavour"):
        pathlibrs.PurePath = pathlibrs.PureWindowsPath
        sys.modules["pathlib"].PurePath = pathlibrs.PureWindowsPath


def pytest_collection_modifyitems(config, items):
    """Mark vendored tests listed in skips.txt with ``@pytest.mark.skip``.

    Matches by MRO so ``PathTest.test_foo`` also skips
    ``WindowsPathTest.test_foo`` (which inherits from PathTest).

    When ``--windows-flavour`` is active, also skips tests that assume
    platform-native ``PurePath`` behaviour (``test_concrete_class``,
    ``test_concrete_parser``) since ``PurePath`` now points to
    ``PureWindowsPath`` regardless of the host OS.
    """
    windows_flavour = config.getoption("--windows-flavour", default=False)

    for item in items:
        if item.cls is None:
            continue

        cls_name = item.cls.__name__
        method_name = item.name

        # Class-level skip (ClassName.*)
        if cls_name in _CLASS_SKIPS:
            item.add_marker(pytest.mark.skip(reason="Not implemented (class-level skip)"))
            continue

        # Method-level skip with MRO matching
        for cls in item.cls.__mro__:
            if (cls.__name__, method_name) in _METHOD_SKIPS:
                item.add_marker(pytest.mark.skip(reason="Listed in tests/skips.txt"))
                break

        # When running with --windows-flavour, skip tests that assume
        # platform-native PurePath class.
        if windows_flavour and method_name in (
            "test_concrete_class",
            "test_concrete_parser",
        ):
            item.add_marker(pytest.mark.skip(reason="Not applicable with --windows-flavour"))
