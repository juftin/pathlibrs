//! Concrete path classes — stubs for Phase 2+.
//!
//! These will add filesystem operations (``.exists()``, ``.open()``, etc.)
//! in a future phase. For now, they are markers that inherit from ``PurePath``.

use std::ffi::OsString;

use pyo3::prelude::*;

use crate::pure::PurePath;

/// Concrete path — will add IO methods in Phase 2.
#[pyclass(extends=PurePath, module = "pathlibrs")]
pub struct Path;

#[pymethods]
impl Path {
    #[new]
    fn new(raw: &str) -> (Self, PurePath) {
        // Default to the platform-native flavour
        #[cfg(windows)]
        let base = PurePath::new_windows(OsString::from(raw));
        #[cfg(not(windows))]
        let base = PurePath::new_posix(OsString::from(raw));

        (Self, base)
    }
}

/// Concrete POSIX path — will add IO methods in Phase 2.
#[pyclass(extends=PurePath, module = "pathlibrs")]
pub struct PosixPath;

#[pymethods]
impl PosixPath {
    #[new]
    fn new(raw: &str) -> (Self, PurePath) {
        (Self, PurePath::new_posix(OsString::from(raw)))
    }
}

/// Concrete Windows path — will add IO methods in Phase 2.
#[pyclass(extends=PurePath, module = "pathlibrs")]
pub struct WindowsPath;

#[pymethods]
impl WindowsPath {
    #[new]
    fn new(raw: &str) -> (Self, PurePath) {
        (Self, PurePath::new_windows(OsString::from(raw)))
    }
}
