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

    // Module metadata
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;

    Ok(())
}
