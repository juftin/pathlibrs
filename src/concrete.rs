//! Concrete path classes — ``PosixPath`` and ``WindowsPath``.
//!
//! These are thin marker classes that extend ``PurePath``.
//! On macOS/Linux, ``Path`` is an alias for ``PosixPath``.
//! All filesystem operations are inherited from ``PurePath``.

use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

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
    #[pyo3(signature = (*args, **kwargs))]
    fn new(
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<(Self, PurePath)> {
        let _ = kwargs;
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
    #[pyo3(signature = (*args, **kwargs))]
    fn new(
        args: &Bound<'_, PyTuple>,
        kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<(Self, PurePath)> {
        let _ = kwargs;
        let raw = join_path_segments(args, PathFlavour::Windows)?;
        Ok((Self, PurePath::new_windows(raw)))
    }
}
