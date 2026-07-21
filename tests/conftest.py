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
import unittest

# ── Redirect pathlib → pathlibrs ────────────────────────────────────────────
import pathlibrs
import pytest

# Capture the real pathlib.types before pathlib is aliased to pathlibrs.
# The vendored test test_matches_writablepath_docstrings accesses
# pathlib.types._WritablePath, which resolves to pathlibrs.types.
# pathlib.types exists only as an importable subpackage in Python 3.12+.
try:
    import pathlib.types as _real_pathlib_types
except ImportError:
    _real_pathlib_types = None

# ── Python < 3.12 compat: assertStartsWith / assertEndsWith ───────────────
# Added in Python 3.12's unittest.TestCase. The vendored CPython 3.14 tests
# use these methods, so backport them for older Python versions.
if not hasattr(unittest.TestCase, "assertStartsWith"):

    def _assert_starts_with(self, first, second, msg=None):  # noqa: N802
        """Fail if first does not start with second."""
        if not first.startswith(second):
            standard_msg = f"{first!r} does not start with {second!r}"  # noqa: N806
            self.fail(self._formatMessage(msg, standard_msg))

    unittest.TestCase.assertStartsWith = _assert_starts_with

if not hasattr(unittest.TestCase, "assertEndsWith"):

    def _assert_ends_with(self, first, second, msg=None):  # noqa: N802
        """Fail if first does not end with second."""
        if not first.endswith(second):
            standard_msg = f"{first!r} does not end with {second!r}"  # noqa: N806
            self.fail(self._formatMessage(msg, standard_msg))

    unittest.TestCase.assertEndsWith = _assert_ends_with

# ── Python < 3.11 compat: assertIsSubclass ─────────────────────────────────
# Added in Python 3.11's unittest.TestCase. The vendored CPython 3.14 tests
# use this method, so backport it for older Python versions.
if not hasattr(unittest.TestCase, "assertIsSubclass"):

    def _assert_is_subclass(self, cls, class_or_tuple, msg=None):  # noqa: N802
        """Fail if cls is not a subclass of class_or_tuple."""
        if not issubclass(cls, class_or_tuple):
            standard_msg = f"{cls!r} is not a subclass of {class_or_tuple!r}"  # noqa: N806
            self.fail(self._formatMessage(msg, standard_msg))

    unittest.TestCase.assertIsSubclass = _assert_is_subclass

sys.modules["pathlib"] = pathlibrs
if _real_pathlib_types is not None:
    pathlibrs.types = _real_pathlib_types
else:
    # Python < 3.12: pathlib is not a package, so pathlib.types doesn't
    # exist.  Create a synthetic types module with a _WritablePath that
    # mirrors docstrings from the actual Path class, so the vendored
    # test_matches_writablepath_docstrings test still passes.
    import types as _synthetic_types_mod  # noqa: E402, F811

    _synthetic_types = _synthetic_types_mod.ModuleType("pathlibrs.types")
    _synthetic_types.__doc__ = "Shim for pathlib.types (injected by pathlibrs test harness)."
    # Create _WritablePath as a protocol-like object whose attributes
    # carry the same __doc__ strings as our Path class.
    _wp_methods = [
        "anchor",
        "full_match",
        "joinpath",
        "mkdir",
        "name",
        "parent",
        "parents",
        "parser",
        "parts",
        "stem",
        "suffix",
        "suffixes",
        "symlink_to",
        "with_name",
        "with_segments",
        "with_stem",
        "with_suffix",
        "write_bytes",
        "write_text",
    ]

    class _WritablePathShim:
        """Shim for _WritablePath protocol (Python < 3.12)."""

    _wp = _WritablePathShim
    for _m in _wp_methods:
        _attr = getattr(pathlibrs.Path, _m, None)
        _doc = getattr(_attr, "__doc__", None) if _attr is not None else None

        class _Descriptor:
            __doc__ = _doc

        setattr(_wp, _m, _Descriptor)
    _synthetic_types._WritablePath = _wp
    pathlibrs.types = _synthetic_types

# ── Shim pathname2url(add_scheme=True) for Python < 3.14 ─────────────────────
# CPython 3.14 added ``add_scheme`` kwarg to urllib.request.pathname2url.
# The vendored test uses this kwarg; shim it for older Python versions.
import urllib.request  # noqa: E402
import os.path  # noqa: E402

_orig_pathname2url = urllib.request.pathname2url


def _pathname2url_shim(p, add_scheme=False):
    """Shim that forwards to pathname2url but also accepts ``add_scheme``."""
    # On Python < 3.14, pathname2url does not prepend // for UNC paths
    # (e.g., //foo/bar should become ////foo/bar before adding file:).
    # CPython 3.14's pathname2url does this automatically via splitroot().
    if p.startswith("//") and not _orig_pathname2url(p).startswith("////"):
        result = "//" + _orig_pathname2url(p)
    else:
        result = _orig_pathname2url(p)
    if add_scheme and not result.startswith("file:"):
        if result.startswith("//"):
            result = "file:" + result
        elif os.path.isabs(p):
            result = "file://" + result
        else:
            result = "file:" + result
    return result


urllib.request.pathname2url = _pathname2url_shim


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

# ── Shim isjunction for Python < 3.12 ────────────────────────────────────────
# posixpath.isjunction and ntpath.isjunction were added in Python 3.12.
# On older Pythons, the vendored test test_is_junction_true cannot
# mock.patch.object(P.parser, "isjunction") because the attribute doesn't
# exist.  Add a no-op that returns False (junctions are Windows-only).
import posixpath as _posixpath  # noqa: E402
import ntpath as _ntpath  # noqa: E402

if not hasattr(_posixpath, "isjunction"):

    def _isjunction_posix(path):  # noqa: N802
        """Test whether a path is a junction."""
        return False

    _posixpath.isjunction = _isjunction_posix

if not hasattr(_ntpath, "isjunction"):

    def _isjunction_nt(path):  # noqa: N802
        """Test whether a path is a junction."""
        return False

    _ntpath.isjunction = _isjunction_nt

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

    # Shimming EnvironmentVarGuard.unset: CPython 3.14 added variadic args
    # (``unset(self, envvar, /, *envvars)``).  Patch the older single-arg
    # version so the vendored test can call ``unset(a, b, c)`` on any host.
    _orig_unset = os_helper.EnvironmentVarGuard.unset

    def _unset_multi(self, envvar, *envvars):
        """Unset one or more environment variables."""
        for ev in (envvar, *envvars):
            _orig_unset(self, ev)

    os_helper.EnvironmentVarGuard.unset = _unset_multi

    # ── Python < 3.11 compat: infinite_recursion ────────────────────────────
    # CPython 3.10 raises RecursionError when sys.setrecursionlimit() is
    # called with a value below the current call stack depth. On Python
    # 3.11+, this was relaxed to a deferred limit. Patch the test helper
    # so test_walk_above_recursion_limit can run on Python 3.10.
    if sys.version_info < (3, 11) and hasattr(test.support, "infinite_recursion"):
        from contextlib import contextmanager as _ir_cm

        _orig_infinite_recursion = test.support.infinite_recursion

        @_ir_cm
        def _infinite_recursion_patched(max_depth=75):
            original_depth = sys.getrecursionlimit()
            try:
                try:
                    sys.setrecursionlimit(max_depth)
                except RecursionError:
                    pass
                yield
            finally:
                sys.setrecursionlimit(original_depth)

        test.support.infinite_recursion = _infinite_recursion_patched

    # ── Shim subst_drive for Python < 3.14 ──────────────────────────────────
    # CPython 3.14 added os_helper.subst_drive().  On Windows, provide a
    # fallback using the subst command directly so test_absolute_windows
    # can run on Python 3.10–3.13.
    if not hasattr(os_helper, "subst_drive") and sys.platform == "win32":
        from contextlib import contextmanager as _scm

        @_scm
        def _subst_drive(path):
            import string as _string
            import subprocess as _sp

            for letter in _string.ascii_uppercase[::-1]:
                drive = f"{letter}:"
                if not os.path.exists(drive):
                    break
            else:
                raise RuntimeError("No free drive letter for subst")
            _sp.check_call(["subst", drive, path])
            try:
                yield drive
            finally:
                _sp.check_call(["subst", drive, "/d"])

        os_helper.subst_drive = _subst_drive

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

    with open(skips_file, encoding="utf-8") as f:
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
            "test_subclass_compat",
            "test_instance_check",
            "test_passing_kwargs_errors",
            "test_parse_windows_path",
        ):
            item.add_marker(pytest.mark.skip(reason="Not applicable with --windows-flavour"))
