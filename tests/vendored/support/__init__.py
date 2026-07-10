"""Compatibility shim for ``test.support`` across Python 3.10–3.14.

The vendored CPython 3.14 test suite imports symbols from ``test.support``
that may not exist in older Python versions. This module re-exports everything
from the real ``test.support`` and stubs any missing symbols.
"""
# Set to 'True' if the tests are run against the pathlib-abc PyPI package.
is_pypi = False

# Re-export everything from the real test.support
from test.support import *  # noqa: F403, E402

# ── Stubs for symbols added after Python 3.10 ──────────────────────────

# Added in Python 3.14
try:
    from test.support import is_wasm32  # noqa: F401
except ImportError:
    is_wasm32 = False
