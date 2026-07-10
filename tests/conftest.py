"""Pytest configuration for pathlibrs vendored CPython test suite.

Redirects ``import pathlib`` → ``import pathlibrs as pathlib`` so the
vendored CPython 3.14 test suite runs against our implementation.
"""
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

# ── Skip tests listed in skips.txt ───────────────────────────────────────────


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


_SKIP_SET = _load_skips()


def pytest_collection_modifyitems(config, items):
    """Mark vendored tests listed in skips.txt with ``@pytest.mark.skip``."""
    for item in items:
        cls_name = item.cls.__name__ if item.cls else ""
        method_name = item.name
        if (cls_name, method_name) in _SKIP_SET:
            item.add_marker(pytest.mark.skip(reason="Listed in tests/skips.txt"))


def pytest_configure(config):
    """Register custom markers."""
    config.addinivalue_line(
        "markers", "skip_vendored: skip a vendored CPython test (from skips.txt)"
    )
