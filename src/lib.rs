//! ``pathlibrs`` — a fast pure-Rust implementation of Python's pathlib.
//!
//! Provides ``PurePath``, ``PurePosixPath``, ``PureWindowsPath``
//! with the same API as CPython's ``pathlib`` module.
//!
//! Phase 1: pure path classes with no filesystem I/O.

pub mod concrete;
pub mod fs;
pub mod iter;
pub mod ops;
pub mod parsing;
pub mod pattern;
pub mod pure;
pub mod repr;

use pyo3::prelude::*;
use std::ffi::OsStr;

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

    // Concrete path classes (stubs for Phase 2+)
    m.add_class::<concrete::Path>()?;
    m.add_class::<concrete::PosixPath>()?;
    m.add_class::<concrete::WindowsPath>()?;

    // Iterators
    m.add_class::<iter::PartsIter>()?;
    m.add_class::<iter::ParentsIter>()?;

    // Set parser class attributes (public API — used by os.fspath)
    let py = m.py();
    let posixpath_mod = py.import("posixpath")?;
    let ntpath_mod = py.import("ntpath")?;

    let pure_posix = m.getattr("PurePosixPath")?;
    pure_posix.setattr("parser", &posixpath_mod)?;
    let pure_path = m.getattr("PurePath")?;
    pure_path.setattr("parser", &posixpath_mod)?;
    let posix_path = m.getattr("PosixPath")?;
    posix_path.setattr("parser", &posixpath_mod)?;
    let path = m.getattr("Path")?;
    path.setattr("parser", &posixpath_mod)?;

    let pure_windows = m.getattr("PureWindowsPath")?;
    pure_windows.setattr("parser", &ntpath_mod)?;
    let windows_path = m.getattr("WindowsPath")?;
    windows_path.setattr("parser", &ntpath_mod)?;

    // Module metadata
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}
