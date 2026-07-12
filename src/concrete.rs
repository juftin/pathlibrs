//! Concrete path classes — ``PosixPath`` and ``WindowsPath``.
//!
//! These are thin marker classes that extend ``PurePath``.
//! On macOS/Linux, ``Path`` is an alias for ``PosixPath``.
//! All filesystem operations are inherited from ``PurePath``.

use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::pure::{join_path_segments, PurePath};
use crate::repr::PathFlavour;

// ═══════════════════════════════════════════════════════════════════════
// PosixPath
// ═══════════════════════════════════════════════════════════════════════

/// Concrete POSIX path with filesystem operations.
#[pyclass(subclass, extends=PurePath, module = "pathlibrs")]
pub struct PosixPath;

#[pymethods]
impl PosixPath {
    #[new]
    #[pyo3(signature = (*args))]
    fn new(args: &Bound<'_, PyTuple>) -> PyResult<(Self, PurePath)> {
        let raw = join_path_segments(args, PathFlavour::Posix)?;
        Ok((Self, PurePath::new_posix(raw)))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// WindowsPath
// ═══════════════════════════════════════════════════════════════════════

/// Concrete Windows path with filesystem operations.
#[pyclass(subclass, extends=PurePath, module = "pathlibrs")]
pub struct WindowsPath;

#[pymethods]
impl WindowsPath {
    #[new]
    #[pyo3(signature = (*args))]
    fn new(args: &Bound<'_, PyTuple>) -> PyResult<(Self, PurePath)> {
        let raw = join_path_segments(args, PathFlavour::Windows)?;
        Ok((Self, PurePath::new_windows(raw)))
    }
}
