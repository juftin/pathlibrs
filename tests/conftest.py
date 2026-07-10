"""Pytest configuration for pathlibrs vendored CPython test suite.

Redirects ``import pathlib`` → ``import pathlibrs as pathlib`` so the
vendored CPython 3.14 test suite runs against our implementation.

The vendored ``tests/vendored/`` directory contains the full CPython 3.14.6
``Lib/test/test_pathlib/`` package, verbatim.  Only ``test_pathlib.py`` is
active in CI; the modular ABC tests (``test_join.py``, ``test_read.py``,
etc.) are vendored for reference but exercise ``pathlib.types`` internals
that pathlibrs does not implement.
"""
import functools
import os
import sys

import pytest

# ── Redirect pathlib → pathlibrs ────────────────────────────────────────────
import pathlibrs

sys.modules["pathlib"] = pathlibrs

# ── Register pathlib._local for Python 3.13 pickle compatibility ───────────
# CPython's Lib/pathlib/_local.py exists so pathlib objects pickled under
# Python 3.13 (which reference ``pathlib._local``) can be unpickled in 3.14+.
# It is just ``from pathlib import *``.  pathlibrs doesn't ship a ``_local``
# submodule, so we inject one dynamically when the vendored test runs.
import types
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
import test.support

if not hasattr(test.support, "is_wasm32"):
    test.support.is_wasm32 = False
if not hasattr(test.support, "is_emscripten"):
    test.support.is_emscripten = False
if not hasattr(test.support, "is_wasi"):
    test.support.is_wasi = False

# ── Patch test.support.os_helper with missing decorators ─────────────────────
import test.support.os_helper as os_helper

if not hasattr(os_helper, "skip_unless_working_chmod"):
    os_helper.skip_unless_working_chmod = lambda fn: fn
if not hasattr(os_helper, "skip_unless_hardlink"):
    os_helper.skip_unless_hardlink = lambda fn: fn
if not hasattr(os_helper, "skip_if_dac_override"):
    os_helper.skip_if_dac_override = lambda fn: fn

# ── Patch test.support.import_helper with missing functions ──────────────────
import test.support.import_helper as import_helper

if not hasattr(import_helper, "ensure_lazy_imports"):

    def _ensure_lazy_imports(module_name, lazy_imports):
        """No-op shim for CPython 3.14's ensure_lazy_imports."""

    import_helper.ensure_lazy_imports = _ensure_lazy_imports


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


def pytest_collection_modifyitems(config, items):
    """Mark vendored tests listed in skips.txt with ``@pytest.mark.skip``.

    Matches by MRO so ``PathTest.test_foo`` also skips
    ``WindowsPathTest.test_foo`` (which inherits from PathTest).
    """
    for item in items:
        if item.cls is None:
            continue

        cls_name = item.cls.__name__
        method_name = item.name

        # Class-level skip (ClassName.*)
        if cls_name in _CLASS_SKIPS:
            item.add_marker(
                pytest.mark.skip(reason="Not implemented (class-level skip)")
            )
            continue

        # Method-level skip with MRO matching
        for cls in item.cls.__mro__:
            if (cls.__name__, method_name) in _METHOD_SKIPS:
                item.add_marker(pytest.mark.skip(reason="Listed in tests/skips.txt"))
                break


def pytest_configure(config):
    """Register custom markers."""
    config.addinivalue_line(
        "markers", "skip_vendored: skip a vendored CPython test (from skips.txt)"
    )
