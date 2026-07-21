//! ``pathlibrs`` — a fast pure-Rust implementation of Python's pathlib.
//!
//! Provides ``PurePath``, ``PurePosixPath``, ``PureWindowsPath``
//! with the same API as CPython's ``pathlib`` module.
//!
//! Phase 1: pure path classes with no filesystem I/O.

pub mod concrete;
pub mod fs;
pub mod glob;
pub mod iter;
pub mod ops;
pub mod parsing;
pub mod pattern;
pub mod pure;
pub mod repr;

use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::ffi::{CString, OsStr};

/// Cross-platform `OsStr::from_bytes` replacement.
///
/// On Unix, `OsStr::from_bytes` exists via `OsStrExt`. On Windows it doesn't.
/// `OsStr::from_encoded_bytes_unchecked` works everywhere. All our call sites
/// pass bytes that originated from a valid `OsStr`, so this is safe.
#[inline]
pub(crate) fn from_os_bytes(bytes: &[u8]) -> &OsStr {
    unsafe { OsStr::from_encoded_bytes_unchecked(bytes) }
}

/// The ``pathlibrs`` Python module.
#[pymodule]
fn pathlibrs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    // Pure path classes (Phase 1)
    m.add_class::<pure::PurePath>()?;
    m.add_class::<pure::PurePosixPath>()?;
    m.add_class::<pure::PureWindowsPath>()?;

    // Cache PurePath type for fast Rust-native construction
    {
        let pure_path_type = m.getattr("PurePath")?;
        pure::init_purepath_type(pure_path_type.unbind());
    }

    // Concrete path classes
    // Path is an alias for the platform-native concrete type
    m.add_class::<concrete::PosixPath>()?;
    m.add_class::<concrete::WindowsPath>()?;

    // On POSIX, Path = PosixPath; on Windows, Path = WindowsPath
    // PurePath is NOT aliased at module level — issubclass checks
    // (PurePosixPath, PurePath) must pass.  On Windows, PurePath.__new__
    // uses PathFlavour::Windows via #[cfg(windows)] instead.
    #[cfg(not(windows))]
    {
        let posix_path = m.getattr("PosixPath")?;
        m.add("Path", posix_path)?;
    }
    #[cfg(windows)]
    {
        let windows_path = m.getattr("WindowsPath")?;
        m.add("Path", windows_path)?;
    }

    // Iterators
    m.add_class::<iter::PartsIter>()?;
    m.add_class::<iter::ParentsIter>()?;
    m.add_class::<iter::GlobIter>()?;
    m.add_class::<iter::IterdirIter>()?;
    m.add_class::<pure::WalkIter>()?;

    // Stat result and PathInfo (Phase 2)
    m.add_class::<fs::StatResult>()?;
    m.add_class::<fs::PathInfo>()?;

    // Set parser class attributes (public API — used by os.fspath)
    let py = m.py();
    let posixpath_mod = py.import("posixpath")?;
    let ntpath_mod = py.import("ntpath")?;

    let pure_posix = m.getattr("PurePosixPath")?;
    pure_posix.setattr("parser", &posixpath_mod)?;
    let pure_path = m.getattr("PurePath")?;
    // PurePath.parser = os.path (platform-native)
    let os_mod = py.import("os")?;
    let os_path = os_mod.getattr("path")?;
    pure_path.setattr("parser", &os_path)?;
    let posix_path = m.getattr("PosixPath")?;
    posix_path.setattr("parser", &posixpath_mod)?;
    // Path = PosixPath on POSIX, so parser is already set
    let path = m.getattr("Path")?;
    path.setattr("parser", &posixpath_mod)?;

    let pure_windows = m.getattr("PureWindowsPath")?;
    pure_windows.setattr("parser", &ntpath_mod)?;
    let windows_path = m.getattr("WindowsPath")?;
    windows_path.setattr("parser", &ntpath_mod)?;

    // UnsupportedOperation — exception raised for unsupported operations
    {
        // Create the class dynamically at module init time.
        // Equivalent to: type('UnsupportedOperation', (NotImplementedError,), {'__doc__': '...'})
        let builtins = py.import("builtins")?;
        let not_impl_error = builtins.getattr("NotImplementedError")?;
        let type_builtin = builtins.getattr("type")?;
        let name = pyo3::types::PyString::new(py, "UnsupportedOperation");
        let bases = pyo3::types::PyTuple::new(py, &[not_impl_error])?;
        let ns = pyo3::types::PyDict::new(py);
        ns.set_item(
            "__doc__",
            "An exception that is raised when an unsupported operation is called.",
        )?;
        ns.set_item("__module__", "pathlibrs")?;
        let unsupported_op = type_builtin.call1((name, bases, ns))?;
        m.add("UnsupportedOperation", unsupported_op)?;
    }

    // Module metadata (added last so it's easy to verify module init ran to end)
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    // Wrap __init__ on all path classes to reject unknown keyword arguments.
    _wrap_path_inits(py, m)?;

    Ok(())
}

/// Wrap `__init__` on path classes to reject unknown kwargs.
///
/// PyO3 with `extends=PyString` does not forward kwargs to `#[new]`
/// or `__init__`, so pure-Python class constructors silently accept
/// invalid keyword arguments.  This monkey-patch restores the
/// CPython behaviour of raising `TypeError`.
fn _wrap_path_inits(py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    let globals = PyDict::new(py);
    globals.set_item("_module", m)?;
    let lines = [
        "def _init_wrapper(cls):",
        "    orig_init = cls.__init__",
        "    def __init__(self, *args, **kwargs):",
        "        if kwargs:",
        "            key = next(iter(kwargs))",
        "            raise TypeError(cls.__name__ + '.__init__() got an unexpected keyword argument ' + repr(key))",
        "        return orig_init(self, *args)",
        "    return __init__",
        "for _n in ('PurePath','PurePosixPath','PureWindowsPath','PosixPath','WindowsPath'):",
        "    getattr(_module, _n).__init__ = _init_wrapper(getattr(_module, _n))",
        "",
    ];
    let code = CString::new(lines.join("\n"))
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("CString: {}", e)))?;
    py.run(&code, Some(&globals), None)?;
    Ok(())
}
