"""Pytest configuration for pathlibrs vendored CPython test suite.

Redirects ``import pathlib`` → ``import pathlibrs as pathlib`` so the
vendored CPython 3.14 test suite runs against our implementation.
"""
import functools
import os
import sys

import pytest

# ── Redirect pathlib → pathlibrs ────────────────────────────────────────────
import pathlibrs

sys.modules["pathlib"] = pathlibrs

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

# ── Patch test.support.import_helper with missing functions ──────────────────
import test.support.import_helper as import_helper

if not hasattr(import_helper, "ensure_lazy_imports"):

    def _ensure_lazy_imports(module_name, lazy_imports):
        """No-op shim for CPython 3.14's ensure_lazy_imports."""

    import_helper.ensure_lazy_imports = _ensure_lazy_imports


# ── Skip tests listed in skips.txt ───────────────────────────────────────────


@functools.lru_cache(maxsize=1)
def _load_skips():
    """Load test skip patterns from skips.txt.

    Returns a set of ``(test_class, test_method)`` tuples to skip.
    """
    skips_file = os.path.join(os.path.dirname(__file__), "skips.txt")
    skips = set()
    if not os.path.exists(skips_file):
        return skips

    with open(skips_file) as f:
        for line in f:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            parts = line.split(None, 1)
            if parts:
                class_method = parts[0]
                if "." in class_method:
                    cls_name, method = class_method.split(".", 1)
                    skips.add((cls_name, method))
    return skips


def pytest_collection_modifyitems(config, items):
    """Mark vendored tests listed in skips.txt with ``@pytest.mark.skip``."""
    skip_set = _load_skips()
    for item in items:
        cls_name = item.cls.__name__ if item.cls else ""
        method_name = item.name
        if (cls_name, method_name) in skip_set:
            item.add_marker(pytest.mark.skip(reason="Listed in tests/skips.txt"))


def pytest_configure(config):
    """Register custom markers."""
    config.addinivalue_line(
        "markers", "skip_vendored: skip a vendored CPython test (from skips.txt)"
    )
