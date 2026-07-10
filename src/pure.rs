//! PyO3 classes: ``PurePath``, ``PurePosixPath``, ``PureWindowsPath``.
//!
//! Implements all Phase 1 properties and methods matching CPython 3.12 pathlib.

use std::ffi::{OsStr, OsString};
use std::hash::{Hash, Hasher};

use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyString, PyTuple};

use crate::iter::ParentsIter;
use crate::ops::{self, stem_from_name, suffix_from_name};
use crate::pattern;
use crate::repr::{PathFlavour, PathRepr};

// ═══════════════════════════════════════════════════════════════════════
// PurePath — base class
// ═══════════════════════════════════════════════════════════════════════

/// Base class for pure (non-IO) path objects.
#[pyclass(subclass, module = "pathlibrs")]
pub struct PurePath {
    pub(crate) inner: PathRepr,
    pub(crate) flavour: PathFlavour,
}

impl PurePath {
    /// Create a new PurePath with POSIX flavour.
    pub fn new_posix(raw: OsString) -> Self {
        Self {
            inner: PathRepr::new(raw),
            flavour: PathFlavour::Posix,
        }
    }

    /// Create a new PurePath with Windows flavour.
    pub fn new_windows(raw: OsString) -> Self {
        Self {
            inner: PathRepr::new(raw),
            flavour: PathFlavour::Windows,
        }
    }

    /// Construct a new path object of the same Python type as `slf_ptr`.
    fn _make_child(
        py: Python<'_>,
        slf_ptr: *mut pyo3::ffi::PyObject,
        new_raw: OsString,
    ) -> PyResult<PyObject> {
        let slf_bound = unsafe { pyo3::Bound::<'_, pyo3::PyAny>::from_borrowed_ptr(py, slf_ptr) };
        let cls = slf_bound.getattr("__class__")?;
        let args = PyTuple::new(py, &[PyString::new(py, &new_raw.to_string_lossy())])?;
        Ok(cls.call1(args)?.unbind())
    }

    #[inline]
    fn _sep(&self) -> u8 {
        match self.flavour {
            PathFlavour::Posix => b'/',
            PathFlavour::Windows => b'\\',
        }
    }

    #[inline]
    fn _is_windows(&self) -> bool {
        self.flavour == PathFlavour::Windows
    }

    fn _anchor_str(&self) -> String {
        let p = self.inner.parsed(self.flavour);
        let mut anchor = String::new();
        if let Some(ref d) = p.drive {
            anchor.push_str(&d.to_string_lossy());
        }
        if let Some(ref r) = p.root {
            anchor.push_str(&r.to_string_lossy());
        }
        anchor
    }

    fn _build_path(
        &self,
        drive: Option<&OsStr>,
        root: Option<&OsStr>,
        parts: &[OsString],
    ) -> OsString {
        let sep = self._sep();
        let mut result = Vec::<u8>::new();
        if let Some(d) = drive {
            result.extend_from_slice(d.as_encoded_bytes());
        }
        if let Some(r) = root {
            result.extend_from_slice(r.as_encoded_bytes());
        }
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                result.push(sep);
            }
            result.extend_from_slice(part.as_encoded_bytes());
        }
        crate::from_os_bytes(&result).to_os_string()
    }

    fn _parent_raw(&self) -> OsString {
        let p = self.inner.parsed(self.flavour);
        if p.parts.is_empty() {
            return self.inner.raw().to_os_string();
        }
        if p.parts.len() == 1 {
            return OsString::from(&self._anchor_str());
        }
        self._build_path(
            p.drive.as_deref(),
            p.root.as_deref(),
            &p.parts[..p.parts.len() - 1],
        )
    }

    fn _str_repr(&self) -> String {
        self.inner.raw().to_string_lossy().into_owned()
    }

    fn _with_name_raw(&self, name: &str) -> OsString {
        let parent_raw = self._parent_raw();
        if parent_raw.as_encoded_bytes().is_empty() {
            OsString::from(name)
        } else {
            let sep = self._sep();
            let mut buf = parent_raw.as_encoded_bytes().to_vec();
            buf.push(sep);
            buf.extend_from_slice(name.as_bytes());
            crate::from_os_bytes(&buf).to_os_string()
        }
    }
}

// -----------------------------------------------------------------------
// pymethods
// -----------------------------------------------------------------------

#[pymethods]
impl PurePath {
    #[new]
    fn new(raw: &str) -> Self {
        Self {
            inner: PathRepr::new(OsString::from(raw)),
            flavour: PathFlavour::Posix,
        }
    }

    // -- properties ----------------------------------------------------

    #[getter]
    fn drive(&self) -> String {
        self.inner
            .parsed(self.flavour)
            .drive
            .as_ref()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    #[getter]
    fn root(&self) -> String {
        self.inner
            .parsed(self.flavour)
            .root
            .as_ref()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    #[getter]
    fn anchor(&self) -> String {
        self._anchor_str()
    }

    #[getter]
    fn name(&self) -> Option<String> {
        let p = self.inner.parsed(self.flavour);
        if !p.has_name {
            return None;
        }
        p.parts.last().map(|s| s.to_string_lossy().into_owned())
    }

    #[getter]
    fn suffix(&self) -> String {
        match self.name() {
            Some(ref n) => suffix_from_name(OsStr::new(n))
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            None => String::new(),
        }
    }

    #[getter]
    fn suffixes(&self) -> Vec<String> {
        match self.name() {
            Some(ref n) => ops::suffixes_from_name(OsStr::new(n))
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect(),
            None => Vec::new(),
        }
    }

    #[getter]
    fn stem(&self) -> String {
        match self.name() {
            Some(ref n) => stem_from_name(OsStr::new(n))
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            None => String::new(),
        }
    }

    #[getter]
    fn parent<'py>(slf: PyRef<'py, Self>) -> PyResult<PyObject> {
        let py = slf.py();
        let ptr = slf.as_ptr();
        let parent_raw = slf._parent_raw();
        PurePath::_make_child(py, ptr, parent_raw)
    }

    #[getter]
    fn parents<'py>(slf: PyRef<'py, Self>) -> PyResult<PyObject> {
        let py = slf.py();
        let cls = {
            let bound =
                unsafe { pyo3::Bound::<'_, pyo3::PyAny>::from_borrowed_ptr(py, slf.as_ptr()) };
            bound.getattr("__class__")?.unbind()
        };
        let iter = ParentsIter::new(&slf.inner, slf.flavour, cls);
        let bound = Py::new(py, iter)?.into_pyobject(py)?;
        Ok(bound.into_any().unbind())
    }

    #[getter]
    fn parts<'py>(slf: PyRef<'py, Self>, py: Python<'py>) -> PyResult<PyObject> {
        let p = slf.inner.parsed(slf.flavour);
        let mut items: Vec<PyObject> = Vec::with_capacity(p.parts.len() + 2);
        items.push(
            p.drive
                .as_ref()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into_pyobject(py)?
                .into(),
        );
        items.push(
            p.root
                .as_ref()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into_pyobject(py)?
                .into(),
        );
        for part in &p.parts {
            items.push(
                part.to_string_lossy()
                    .into_owned()
                    .into_pyobject(py)?
                    .into(),
            );
        }
        let tuple = PyTuple::new(py, items)?;
        Ok(tuple.into())
    }

    // -- methods -------------------------------------------------------

    #[pyo3(signature = (*args))]
    fn joinpath<'py>(slf: PyRef<'py, Self>, args: &Bound<'py, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let ptr = slf.as_ptr();
        let mut result = slf.inner.raw().to_os_string();

        if let Ok(tuple) = args.downcast::<PyTuple>() {
            for arg in tuple.iter() {
                let s: String = arg.extract()?;
                if !s.is_empty() {
                    if result.as_encoded_bytes().is_empty() {
                        result = OsString::from(&s);
                    } else {
                        let sep = slf._sep();
                        let mut buf = result.as_encoded_bytes().to_vec();
                        buf.push(sep);
                        buf.extend_from_slice(s.as_bytes());
                        result = crate::from_os_bytes(&buf).to_os_string();
                    }
                }
            }
        }
        PurePath::_make_child(py, ptr, result)
    }

    fn with_name<'py>(slf: PyRef<'py, Self>, name: &str) -> PyResult<PyObject> {
        let py = slf.py();
        let ptr = slf.as_ptr();
        let new_raw = slf._with_name_raw(name);
        PurePath::_make_child(py, ptr, new_raw)
    }

    fn with_stem<'py>(slf: PyRef<'py, Self>, stem: &str) -> PyResult<PyObject> {
        let name = slf.name().unwrap_or_default();
        let old_suffix = suffix_from_name(OsStr::new(&name))
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let new_name = format!("{}{}", stem, old_suffix);
        PurePath::with_name(slf, &new_name)
    }

    fn with_suffix<'py>(slf: PyRef<'py, Self>, suffix: &str) -> PyResult<PyObject> {
        let name = slf.name().unwrap_or_default();
        let old_stem = stem_from_name(OsStr::new(&name))
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| name.clone());
        let new_name = if suffix.is_empty() {
            old_stem
        } else {
            format!("{}{}", old_stem, suffix)
        };
        PurePath::with_name(slf, &new_name)
    }

    fn relative_to<'py>(slf: PyRef<'py, Self>, other: &Bound<'py, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let ptr = slf.as_ptr();
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), slf.flavour);
        let self_parsed = slf.inner.parsed(slf.flavour);

        if self_parsed.drive != other_parsed.drive
            || self_parsed.root != other_parsed.root
            || self_parsed.parts.len() < other_parsed.parts.len()
        {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "'{}' does not start with '{}'",
                slf._str_repr(),
                other_str
            )));
        }
        for i in 0..other_parsed.parts.len() {
            if self_parsed.parts[i] != other_parsed.parts[i] {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "'{}' does not start with '{}'",
                    slf._str_repr(),
                    other_str
                )));
            }
        }

        let remaining = &self_parsed.parts[other_parsed.parts.len()..];
        let sep = slf._sep();
        let mut buf = Vec::<u8>::new();
        for (i, part) in remaining.iter().enumerate() {
            if i > 0 {
                buf.push(sep);
            }
            buf.extend_from_slice(part.as_encoded_bytes());
        }
        let new_raw = if buf.is_empty() {
            OsString::from(".")
        } else {
            crate::from_os_bytes(&buf).to_os_string()
        };
        PurePath::_make_child(py, ptr, new_raw)
    }

    fn is_relative_to(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), self.flavour);
        let self_parsed = self.inner.parsed(self.flavour);
        if self_parsed.drive != other_parsed.drive
            || self_parsed.root != other_parsed.root
            || self_parsed.parts.len() < other_parsed.parts.len()
        {
            return Ok(false);
        }
        for i in 0..other_parsed.parts.len() {
            if self_parsed.parts[i] != other_parsed.parts[i] {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn as_posix(&self) -> String {
        let raw = self.inner.raw().as_encoded_bytes();
        let mut result = Vec::with_capacity(raw.len());
        for &b in raw {
            result.push(if b == b'\\' { b'/' } else { b });
        }
        String::from_utf8_lossy(&result).into_owned()
    }

    fn as_uri(&self) -> PyResult<String> {
        let p = self.inner.parsed(self.flavour);
        match self.flavour {
            PathFlavour::Posix => {
                if p.root.is_some() {
                    Ok(format!("file://{}", self.as_posix()))
                } else {
                    Ok(format!("file:{}", self.as_posix()))
                }
            }
            PathFlavour::Windows => {
                if let Some(ref drive) = p.drive {
                    let drive_str = drive.to_string_lossy();
                    if drive_str.starts_with("\\\\") {
                        let trimmed = drive_str
                            .replace('\\', "/")
                            .trim_start_matches('/')
                            .to_string();
                        let rest: Vec<u8> = self.inner.raw().as_encoded_bytes()[p.anchor_length..]
                            .iter()
                            .map(|&b| if b == b'\\' { b'/' } else { b })
                            .collect();
                        let rest_str = String::from_utf8_lossy(&rest)
                            .trim_start_matches('/')
                            .to_string();
                        Ok(format!("file://{}/{}", trimmed, rest_str))
                    } else {
                        let drive_letter = drive_str.trim_end_matches(':');
                        let rest: Vec<u8> = self.inner.raw().as_encoded_bytes()[p.anchor_length..]
                            .iter()
                            .map(|&b| if b == b'\\' { b'/' } else { b })
                            .collect();
                        let rest_str = String::from_utf8_lossy(&rest)
                            .trim_start_matches('/')
                            .to_string();
                        Ok(format!("file:///{}:/{}", drive_letter, rest_str))
                    }
                } else {
                    Ok(format!("file:{}", self.as_posix()))
                }
            }
        }
    }

    #[pyo3(name = "match")]
    fn match_(&self, pattern: &str) -> bool {
        pattern::match_path(
            OsStr::new(pattern),
            self.inner.raw(),
            true,
            self._is_windows(),
        )
    }

    // -- dunder methods ------------------------------------------------

    fn __truediv__<'py>(slf: PyRef<'py, Self>, other: &Bound<'py, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let ptr = slf.as_ptr();
        let other_str = _extract_path_str(other)?;
        let mut raw = slf.inner.raw().to_os_string();
        if !raw.as_encoded_bytes().is_empty() && !other_str.is_empty() {
            let sep = slf._sep();
            let mut buf = raw.as_encoded_bytes().to_vec();
            buf.push(sep);
            buf.extend_from_slice(other_str.as_bytes());
            raw = crate::from_os_bytes(&buf).to_os_string();
        } else if raw.as_encoded_bytes().is_empty() {
            raw = OsString::from(&other_str);
        }
        PurePath::_make_child(py, ptr, raw)
    }

    fn __rtruediv__<'py>(slf: PyRef<'py, Self>, other: &Bound<'py, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let ptr = slf.as_ptr();
        let other_str = _extract_path_str(other)?;
        let path_raw = slf.inner.raw().to_os_string();
        let raw = if other_str.is_empty() {
            path_raw
        } else if path_raw.as_encoded_bytes().is_empty() {
            OsString::from(&other_str)
        } else {
            let sep = slf._sep();
            let mut buf = other_str.as_bytes().to_vec();
            buf.push(sep);
            buf.extend_from_slice(path_raw.as_encoded_bytes());
            crate::from_os_bytes(&buf).to_os_string()
        };
        PurePath::_make_child(py, ptr, raw)
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), self.flavour);
        Ok(self.inner.parsed(self.flavour) == &other_parsed)
    }

    fn __hash__(&self) -> u64 {
        let p = self.inner.parsed(self.flavour);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        p.hash(&mut hasher);
        (self.flavour as u8).hash(&mut hasher);
        hasher.finish()
    }

    fn __lt__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other_str = _extract_path_str(other)?;
        Ok(self.inner.raw().as_encoded_bytes() < other_str.as_bytes())
    }

    fn __str__(&self) -> String {
        self.inner.raw().to_string_lossy().into_owned()
    }

    fn __repr__(&self) -> String {
        let class_name = match self.flavour {
            PathFlavour::Posix => "PurePosixPath",
            PathFlavour::Windows => "PureWindowsPath",
        };
        format!("{}('{}')", class_name, self._str_repr())
    }

    fn __fspath__(&self) -> String {
        self.inner.raw().to_string_lossy().into_owned()
    }

    fn __reduce__<'py>(slf: PyRef<'py, Self>, py: Python<'py>) -> PyResult<PyObject> {
        let bound = unsafe { pyo3::Bound::<'_, pyo3::PyAny>::from_borrowed_ptr(py, slf.as_ptr()) };
        let cls = bound.getattr("__class__")?;
        let raw = slf.inner.raw().to_string_lossy().into_owned();
        let args = PyTuple::new(py, &[PyString::new(py, &raw)])?;
        let elements: Vec<Bound<'py, pyo3::PyAny>> = vec![cls, args.into_any()];
        let reduce = PyTuple::new(py, elements)?;
        Ok(reduce.into_any().unbind())
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PurePosixPath
// ═══════════════════════════════════════════════════════════════════════

#[pyclass(extends=PurePath, module = "pathlibrs")]
pub struct PurePosixPath;

#[pymethods]
impl PurePosixPath {
    #[new]
    fn new(raw: &str) -> (Self, PurePath) {
        (Self, PurePath::new_posix(OsString::from(raw)))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PureWindowsPath
// ═══════════════════════════════════════════════════════════════════════

#[pyclass(extends=PurePath, module = "pathlibrs")]
pub struct PureWindowsPath;

#[pymethods]
impl PureWindowsPath {
    #[new]
    fn new(raw: &str) -> (Self, PurePath) {
        (Self, PurePath::new_windows(OsString::from(raw)))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// helpers
// ═══════════════════════════════════════════════════════════════════════

/// Extract a string from a Python object.
fn _extract_path_str(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    if let Ok(s) = obj.extract::<String>() {
        return Ok(s);
    }
    Ok(obj.str()?.to_string())
}
