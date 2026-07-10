"""Pytest configuration for pathlibrs vendored CPython test suite.

Redirects ``import pathlib`` → ``import pathlibrs as pathlib`` so the
vendored CPython 3.14 test suite runs against our implementation.

The vendored ``tests/vendored/`` directory contains the full CPython 3.14.6
``Lib/test/test_pathlib/`` package, verbatim.  Only ``test_pathlib.py`` is
active in CI; the modular ABC tests (``test_join.py``, ``test_read.py``,
etc.) are vendored for reference but exercise ``pathlib.types`` internals
that pathlibrs does not implement.
"""
import os
import sys

import pytest

# ── Redirect pathlib → pathlibrs ────────────────────────────────────────────
import pathlibrs

sys.modules["pathlib"] = pathlibrs

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

# ── Skip list management ────────────────────────────────────────────────────


def _load_skips() -> tuple[set[tuple[str, str]], set[str]]:
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
    """Mark vendored tests listed in skips.txt with ``@pytest.mark.skip``."""
    for item in items:
        cls_name = item.cls.__name__ if item.cls else ""

        # Class-level skip (ClassName.*)
        if cls_name in _CLASS_SKIPS:
            item.add_marker(
                pytest.mark.skip(reason="Not implemented (class-level skip)")
            )
            continue

        # Method-level skip (ClassName.method_name)
        method_name = item.name
        if (cls_name, method_name) in _METHOD_SKIPS:
            item.add_marker(pytest.mark.skip(reason="Listed in tests/skips.txt"))


def pytest_configure(config):
    """Register custom markers."""
    config.addinivalue_line(
        "markers", "skip_vendored: skip a vendored CPython test (from skips.txt)"
    )
