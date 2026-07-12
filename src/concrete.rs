//! Concrete path classes — ``PosixPath`` and ``WindowsPath``.
//!
//! These are thin marker classes that extend ``PurePath``.
//! On macOS/Linux, ``Path`` is an alias for ``PosixPath``.
//! All filesystem operations are inherited from ``PurePath``.

use std::ffi::OsString;

use pyo3::prelude::*;
use pyo3::types::PyTuple;

use crate::pure::PurePath;

// ═══════════════════════════════════════════════════════════════════════
// PosixPath
// ═══════════════════════════════════════════════════════════════════════

/// Concrete POSIX path with filesystem operations.
#[pyclass(subclass, extends=PurePath, module = "pathlibrs")]
pub struct PosixPath;

/// Extract a path string from a Python object, handling lone surrogates
/// via lossy conversion (replacing invalid UTF-8 with replacement chars).
fn _extract_os_str(obj: &Bound<'_, pyo3::PyAny>) -> PyResult<OsString> {
    use pyo3::types::PyBytes;
    // Reject bytes arguments (CPython pathlib raises TypeError for bytes).
    if obj.is_instance_of::<PyBytes>() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "argument should be a str or an os.PathLike object where __fspath__ returns a str, not 'bytes'",
        ));
    }
    // Try extracting as String first (valid UTF-8 path)
    if let Ok(s) = obj.extract::<String>() {
        return Ok(OsString::from(s));
    }
    // Handle strings with lone surrogates: use Python-level encoding
    if let Ok(py_str) = obj.downcast::<pyo3::types::PyString>() {
        // Use Python's utf-8 encoding with surrogatepass to get raw bytes
        let encoded = py_str.call_method1("encode", ("utf-8", "surrogatepass"))?;
        let bytes = encoded.downcast::<PyBytes>()?.as_bytes();
        return Ok(crate::from_os_bytes(bytes).to_os_string());
    }
    // Other path-like objects
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "expected str, bytes or os.PathLike object, not '{}'",
        obj.get_type().name()?
    )))
}

#[pymethods]
impl PosixPath {
    #[new]
    #[pyo3(signature = (*args))]
    fn new(args: &Bound<'_, PyTuple>) -> PyResult<(Self, PurePath)> {
        let raw: OsString = if args.is_empty() {
            OsString::from(".")
        } else if args.len() == 1 {
            _extract_os_str(&args.get_item(0)?)?
        } else {
            let parts: Vec<OsString> = args
                .iter()
                .map(|item| _extract_os_str(&item))
                .collect::<PyResult<Vec<OsString>>>()?;
            let joined: String = parts
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            OsString::from(joined)
        };
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
        let raw: OsString = if args.is_empty() {
            OsString::from(".")
        } else if args.len() == 1 {
            _extract_os_str(&args.get_item(0)?)?
        } else {
            let parts: Vec<OsString> = args
                .iter()
                .map(|item| _extract_os_str(&item))
                .collect::<PyResult<Vec<OsString>>>()?;
            let joined: String = parts
                .iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
                .join("/");
            OsString::from(joined)
        };
        Ok((Self, PurePath::new_windows(raw)))
    }
}
