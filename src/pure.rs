//! PyO3 classes: ``PurePath``, ``PurePosixPath``, ``PureWindowsPath``.
//!
//! Implements all Phase 1 properties and methods matching CPython 3.12+ pathlib.

use std::ffi::{OsStr, OsString};
use std::hash::{Hash, Hasher};
use std::sync::Mutex;

use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyList, PyString, PyTuple, PyType};

use crate::fs::PathInfo;
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
    pub(crate) path_info: Mutex<Option<Py<PathInfo>>>,
}

impl PurePath {
    /// Create a new PurePath with POSIX flavour.
    pub fn new_posix(raw: OsString) -> Self {
        Self {
            inner: PathRepr::new(raw),
            flavour: PathFlavour::Posix,
            path_info: Mutex::new(None),
        }
    }

    /// Create a new PurePath with Windows flavour.
    pub fn new_windows(raw: OsString) -> Self {
        Self {
            inner: PathRepr::new(raw),
            flavour: PathFlavour::Windows,
            path_info: Mutex::new(None),
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
            let anchor = self._anchor_str();
            if anchor.is_empty() {
                return OsString::from(".");
            }
            return OsString::from(&anchor);
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
    #[pyo3(signature = (*args))]
    fn new(args: &Bound<'_, PyTuple>) -> PyResult<Self> {
        let raw = join_path_segments(args, PathFlavour::Posix)?;
        Ok(Self {
            inner: PathRepr::new(raw),
            #[cfg(windows)]
            flavour: PathFlavour::Windows,
            #[cfg(not(windows))]
            flavour: PathFlavour::Posix,
            path_info: Mutex::new(None),
        })
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
                let s = _extract_path_str(&arg)?;
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
        if slf.name().is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "'{}' has an empty name",
                slf._str_repr()
            )));
        }
        // Reject invalid characters in the new name.
        // On Windows, a bare ":" is invalid (looks like a drive separator),
        // but "d:" or "d:e" are valid NTFS stream names.
        // Path separators and null bytes are forbidden on all platforms.
        if name == ":" {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid name '{name}'"
            )));
        }
        if name.contains('\0') || name.contains('/') || name.contains('\\') {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid name '{name}'"
            )));
        }
        let py = slf.py();
        let ptr = slf.as_ptr();
        let new_raw = slf._with_name_raw(name);
        PurePath::_make_child(py, ptr, new_raw)
    }

    fn with_stem<'py>(slf: PyRef<'py, Self>, stem: &str) -> PyResult<PyObject> {
        if slf.name().is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "'{}' has an empty name",
                slf._str_repr()
            )));
        }
        let name = slf.name().unwrap_or_default();
        let old_suffix = suffix_from_name(OsStr::new(&name))
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let new_name = format!("{stem}{old_suffix}");
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
            format!("{old_stem}{suffix}")
        };
        PurePath::with_name(slf, &new_name)
    }

    /// ``_parse_path(raw_path)`` — class method.
    ///
    /// Parse a raw path string into ``(drive, root, parts)``.
    /// The flavour is determined from the class's ``parser`` attribute.
    #[classmethod]
    #[pyo3(signature = (raw_path))]
    fn _parse_path(_cls: &Bound<'_, PyType>, raw_path: &str) -> PyResult<PyObject> {
        let py = _cls.py();
        let flavour = if _cls
            .getattr("parser")?
            .getattr("sep")?
            .extract::<String>()?
            == "/"
        {
            PathFlavour::Posix
        } else {
            PathFlavour::Windows
        };
        let parsed = crate::parsing::parse_path(OsStr::new(raw_path), flavour);
        let drive: PyObject = parsed
            .drive
            .as_ref()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default()
            .into_pyobject(py)?
            .into_any()
            .unbind();
        let root: PyObject = parsed
            .root
            .as_ref()
            .map(|r| r.to_string_lossy().into_owned())
            .unwrap_or_default()
            .into_pyobject(py)?
            .into_any()
            .unbind();
        let parts_list = {
            let items: Vec<PyObject> = parsed
                .parts
                .iter()
                .map(|p| {
                    p.to_string_lossy()
                        .into_owned()
                        .into_pyobject(py)
                        .unwrap()
                        .into_any()
                        .unbind()
                })
                .collect();
            PyList::new(py, items)?.into_any().unbind()
        };
        let result = PyTuple::new(py, [drive, root, parts_list])?;
        Ok(result.into_any().unbind())
    }

    /// ``with_segments(*pathsegments)`` — class method.
    ///
    /// Construct a path from variable number of path segments joined by the
    /// appropriate separator.
    #[classmethod]
    #[pyo3(signature = (*pathsegments))]
    fn with_segments(
        _cls: &Bound<'_, PyType>,
        pathsegments: &Bound<'_, PyTuple>,
    ) -> PyResult<PyObject> {
        let _py = _cls.py();
        let parts: Vec<String> = pathsegments
            .iter()
            .map(|item| item.extract::<String>())
            .collect::<PyResult<Vec<String>>>()?;

        let segments_str = parts.join("/");
        Ok(_cls.call1((segments_str,))?.unbind())
    }

    /// ``from_uri(uri)`` — class method.
    ///
    /// Construct a path from a ``file:`` URI. The inverse of ``as_uri()``.
    #[classmethod]
    #[pyo3(signature = (uri))]
    fn from_uri(_cls: &Bound<'_, PyType>, uri: &str) -> PyResult<PyObject> {
        let _py = _cls.py();
        let path_str = parse_file_uri(uri)?;
        Ok(_cls.call1((path_str,))?.unbind())
    }

    #[pyo3(signature = (other, *, walk_up = false))]
    fn relative_to<'py>(
        slf: PyRef<'py, Self>,
        other: &Bound<'py, PyAny>,
        walk_up: bool,
    ) -> PyResult<PyObject> {
        let py = slf.py();
        let ptr = slf.as_ptr();
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), slf.flavour);
        let self_parsed = slf.inner.parsed(slf.flavour);

        // Find how many leading segments match
        let min_len = self_parsed.parts.len().min(other_parsed.parts.len());
        let mut common = 0usize;

        if !_drives_equal(&self_parsed.drive, &other_parsed.drive, slf._is_windows())
            || self_parsed.root != other_parsed.root
        {
            // Anchors differ — no common prefix at all
            if !walk_up {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "'{}' does not start with '{}'",
                    slf._str_repr(),
                    other_str
                )));
            }
            // With walk_up=True, different anchors produce all ".." segments
        } else {
            for i in 0..min_len {
                if crate::repr::ParsedPath::parts_equal(
                    &self_parsed.parts[i],
                    &other_parsed.parts[i],
                    slf._is_windows(),
                ) {
                    common += 1;
                } else {
                    break;
                }
            }
        }

        if !walk_up && common < other_parsed.parts.len() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "'{}' does not start with '{}'",
                slf._str_repr(),
                other_str
            )));
        }

        if walk_up {
            // Number of ".." segments = number of non-matching parts in other
            let remaining_in_other = other_parsed.parts.len() - common;
            let remaining_in_self = &self_parsed.parts[common..];

            let mut bufs: Vec<String> =
                Vec::with_capacity(remaining_in_other + remaining_in_self.len());
            for _ in 0..remaining_in_other {
                bufs.push("..".to_string());
            }
            for part in remaining_in_self {
                bufs.push(part.to_string_lossy().into_owned());
            }

            let new_raw = if bufs.is_empty() {
                OsString::from(".")
            } else {
                OsString::from(bufs.join("/"))
            };
            PurePath::_make_child(py, ptr, new_raw)
        } else {
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
    }

    fn is_relative_to(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), self.flavour);
        let self_parsed = self.inner.parsed(self.flavour);
        if !_drives_equal(&self_parsed.drive, &other_parsed.drive, self._is_windows())
            || self_parsed.root != other_parsed.root
            || self_parsed.parts.len() < other_parsed.parts.len()
        {
            return Ok(false);
        }
        for i in 0..other_parsed.parts.len() {
            if !crate::repr::ParsedPath::parts_equal(
                &self_parsed.parts[i],
                &other_parsed.parts[i],
                self._is_windows(),
            ) {
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
        // Emit DeprecationWarning — PurePath.as_uri() is deprecated
        // in favor of concrete Path.as_uri() (CPython compat).
        Python::with_gil(|py| {
            let _ = py.import("warnings")?.call_method1(
                "warn",
                (
                    "PurePath.as_uri() is deprecated, use Path.as_uri() instead",
                    py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),
                ),
            );
            Ok::<_, PyErr>(())
        })?;
        let p = self.inner.parsed(self.flavour);
        // Non-absolute paths on Windows cannot produce a file: URI
        if self.flavour == PathFlavour::Windows {
            if p.drive.is_none() {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "path '{}' is not absolute on Windows",
                    self._str_repr()
                )));
            }
            if p.root.is_none() {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "path '{}' is not absolute on Windows",
                    self._str_repr()
                )));
            }
        }
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
                        let rest = self.as_posix()[p.anchor_length..]
                            .trim_start_matches('/')
                            .to_string();
                        if rest.is_empty() {
                            Ok(format!("file://{}/", trimmed))
                        } else {
                            Ok(format!("file://{}/{}", trimmed, rest))
                        }
                    } else {
                        let drive_letter = drive_str.trim_end_matches(':');
                        let rest = self.as_posix()[p.anchor_length..]
                            .trim_start_matches('/')
                            .to_string();
                        Ok(format!("file:///{}:/{}", drive_letter, rest))
                    }
                } else {
                    Ok(format!("file:{}", self.as_posix()))
                }
            }
        }
    }

    #[pyo3(name = "match")]
    #[pyo3(signature = (pattern, *, case_sensitive = None))]
    fn match_(&self, pattern: &str, case_sensitive: Option<bool>) -> bool {
        let cs = case_sensitive.unwrap_or(!self._is_windows());
        let is_windows = self._is_windows();
        // On Windows, patterns like "*:" or "c:" prefix a drive component.
        // Strip the drive from both pattern and path before matching, then
        // verify the drive matches separately.
        // The pattern and path must agree on whether a root follows the drive.
        if is_windows {
            if let Some((pat_drive, pat_root, pat_rest)) = _split_drive_from_pattern(pattern) {
                let self_raw = self.inner.raw().to_string_lossy();
                if let Some((path_drive, path_root, path_rest)) = _split_drive_from_path(&self_raw)
                {
                    // Root presence must match
                    if pat_root != path_root {
                        return false;
                    }
                    // Match drive with fnmatch, then match the rest
                    if !pattern::fnmatch_bytes(pat_drive.as_bytes(), path_drive.as_bytes(), cs) {
                        return false;
                    }
                    return pattern::match_path(
                        OsStr::new(pat_rest),
                        OsStr::new(path_rest),
                        cs,
                        is_windows,
                    );
                }
            }
        }
        pattern::match_path(OsStr::new(pattern), self.inner.raw(), cs, is_windows)
    }

    /// ``full_match(pattern, *, case_sensitive=None)``
    ///
    /// Like ``match()`` but the pattern must match the *entire* path.
    /// A relative pattern like ``"*.py"`` will NOT match ``"/a/b/foo.py"``.
    #[pyo3(name = "full_match")]
    #[pyo3(signature = (pattern, *, case_sensitive = None))]
    fn full_match_(&self, pattern: &str, case_sensitive: Option<bool>) -> bool {
        let cs = case_sensitive.unwrap_or(!self._is_windows());
        pattern::full_match_path(
            OsStr::new(pattern),
            self.inner.raw(),
            cs,
            self._is_windows(),
        )
    }

    // -- filesystem properties (Phase 2) -----------------------------

    /// Return stat information for this path.
    #[pyo3(signature = (*, follow_symlinks = true))]
    fn stat<'py>(slf: PyRef<'py, Self>, follow_symlinks: bool) -> PyResult<PyObject> {
        let py = slf.py();
        let st = crate::fs::stat(slf.inner.raw(), follow_symlinks)?;
        Ok(Py::new(py, st)?.into_pyobject(py)?.into_any().unbind())
    }

    /// Return stat information without following symlinks.
    fn lstat<'py>(slf: PyRef<'py, Self>) -> PyResult<PyObject> {
        let py = slf.py();
        let st = crate::fs::stat(slf.inner.raw(), false)?;
        Ok(Py::new(py, st)?.into_pyobject(py)?.into_any().unbind())
    }

    /// Check whether the path exists.
    #[pyo3(signature = (*, follow_symlinks = true))]
    fn exists(&self, follow_symlinks: bool) -> PyResult<bool> {
        crate::fs::exists(self.inner.raw(), follow_symlinks)
    }

    /// Check whether the path is a directory.
    #[pyo3(signature = (*, follow_symlinks = true))]
    fn is_dir(&self, follow_symlinks: bool) -> PyResult<bool> {
        match crate::fs::stat_if_exists(self.inner.raw(), follow_symlinks) {
            Some(st) => Ok((st.st_mode & 0o170000) == 0o040000),
            None => Ok(false),
        }
    }

    /// Check whether the path is a regular file.
    #[pyo3(signature = (*, follow_symlinks = true))]
    fn is_file(&self, follow_symlinks: bool) -> PyResult<bool> {
        match crate::fs::stat_if_exists(self.inner.raw(), follow_symlinks) {
            Some(st) => Ok((st.st_mode & 0o170000) == 0o100000),
            None => Ok(false),
        }
    }

    /// Check whether the path is a symbolic link.
    fn is_symlink(&self) -> PyResult<bool> {
        match crate::fs::stat_if_exists(self.inner.raw(), false) {
            Some(st) => Ok((st.st_mode & 0o170000) == 0o120000),
            None => Ok(false),
        }
    }

    /// Check whether the path is a junction (Windows only; always False on POSIX).
    #[allow(deprecated)]
    fn is_junction<'py>(slf: PyRef<'py, Self>) -> PyResult<PyObject> {
        let raw_str = slf.inner.raw().to_string_lossy();
        let py = slf.py();
        if raw_str.contains('\u{fffd}') || raw_str.contains('\x00') {
            return Ok(false.into_py(py));
        }
        // Delegate to parser.isjunction if available (matching CPython behavior)
        let slf_bound =
            unsafe { pyo3::Bound::<'_, pyo3::PyAny>::from_borrowed_ptr(py, slf.as_ptr()) };
        if let Ok(parser) = slf_bound.getattr("parser") {
            if let Ok(result) = parser.call_method1("isjunction", (&slf_bound,)) {
                return Ok(result.unbind());
            }
        }
        // On POSIX, isjunction is not available — return False
        Ok(false.into_py(py))
    }

    /// Check whether the path is a mount point.
    fn is_mount(&self) -> PyResult<bool> {
        crate::fs::is_mount(self.inner.raw())
    }

    /// Check whether the path is a block device.
    fn is_block_device(&self) -> PyResult<bool> {
        match crate::fs::stat(self.inner.raw(), false) {
            Ok(st) => Ok((st.st_mode & 0o170000) == 0o060000),
            Err(_) => Ok(false),
        }
    }

    /// Check whether the path is a character device.
    fn is_char_device(&self) -> PyResult<bool> {
        match crate::fs::stat(self.inner.raw(), false) {
            Ok(st) => Ok((st.st_mode & 0o170000) == 0o020000),
            Err(_) => Ok(false),
        }
    }

    /// Check whether the path is a FIFO (named pipe).
    fn is_fifo(&self) -> PyResult<bool> {
        match crate::fs::stat(self.inner.raw(), false) {
            Ok(st) => Ok((st.st_mode & 0o170000) == 0o010000),
            Err(_) => Ok(false),
        }
    }

    /// Check whether the path is a Unix socket.
    fn is_socket(&self) -> PyResult<bool> {
        match crate::fs::stat(self.inner.raw(), false) {
            Ok(st) => Ok((st.st_mode & 0o170000) == 0o140000),
            Err(_) => Ok(false),
        }
    }

    /// Check whether this path points to the same file as *other*.
    fn samefile(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other_str = _extract_path_str(other)?;
        crate::fs::samefile(self.inner.raw(), OsStr::new(&other_str))
    }

    /// Return the user name of the file owner.
    #[pyo3(signature = (*, follow_symlinks = true))]
    fn owner(&self, follow_symlinks: bool) -> PyResult<String> {
        crate::fs::owner(self.inner.raw(), follow_symlinks)
    }

    /// Return the group name of the file.
    #[pyo3(signature = (*, follow_symlinks = true))]
    fn group(&self, follow_symlinks: bool) -> PyResult<String> {
        crate::fs::group(self.inner.raw(), follow_symlinks)
    }

    /// Resolve the path to an absolute path, resolving symlinks.
    #[pyo3(signature = (*, strict = false))]
    fn resolve<'py>(slf: PyRef<'py, Self>, strict: bool) -> PyResult<PyObject> {
        let py = slf.py();
        let resolved = crate::fs::resolve(slf.inner.raw(), strict)?;
        Self::_make_child(py, slf.as_ptr(), OsString::from(resolved.as_os_str()))
    }

    /// Return an absolute version of this path (no symlink resolution).
    ///
    /// Uses ``os.getcwd()`` so that tests can mock it.
    /// When the path is ``"."``, returns the cwd directly without a trailing ``/.``
    /// (matching CPython behavior).
    fn absolute<'py>(slf: PyRef<'py, Self>) -> PyResult<PyObject> {
        let py = slf.py();
        let raw = slf.inner.raw();
        let raw_str = raw.to_string_lossy();

        if std::path::Path::new(raw).is_absolute() {
            return Self::_make_child(py, slf.as_ptr(), OsString::from(raw));
        }

        // Use Python's os.getcwd() so tests can mock it
        let os_mod = py.import("os")?;
        let cwd: String = os_mod.call_method0("getcwd")?.extract()?;

        // When the raw path is ".", just return the cwd without trailing "/."
        // This matches CPython's os.path.join(cwd, ".") = cwd
        let result = if raw_str.as_ref() == "." {
            OsString::from(&cwd)
        } else {
            // Push components individually through PathBuf to normalize
            // separators on Windows (where "a/b/c" must become "a\\b\\c").
            let mut combined = std::path::PathBuf::from(&cwd);
            for component in std::path::Path::new(raw).components() {
                combined.push(component.as_os_str());
            }
            OsString::from(combined.as_os_str())
        };
        Self::_make_child(py, slf.as_ptr(), result)
    }

    /// Return the target of this symlink as a new Path.
    fn readlink<'py>(slf: PyRef<'py, Self>) -> PyResult<PyObject> {
        let py = slf.py();
        let target = crate::fs::readlink_raw(slf.inner.raw())?;
        Self::_make_child(py, slf.as_ptr(), OsString::from(target.as_os_str()))
    }

    /// Return the current working directory as a Path.
    #[classmethod]
    fn cwd(_cls: &Bound<'_, PyType>) -> PyResult<PyObject> {
        let cwd = std::env::current_dir()
            .map_err(|e| pyo3::exceptions::PyOSError::new_err(e.to_string()))?;
        let cwd_str = cwd.to_string_lossy().to_string();
        Ok(_cls.call1((cwd_str,))?.unbind())
    }

    /// Return the home directory as a Path.
    #[classmethod]
    fn home(_cls: &Bound<'_, PyType>) -> PyResult<PyObject> {
        let py = _cls.py();
        let os_path = py.import("os.path")?;
        let home = os_path.call_method1("expanduser", ("~",))?;
        let home_str: String = home.extract()?;
        Ok(_cls.call1((home_str,))?.unbind())
    }

    /// Expand ``~`` and ``~user`` in the path.
    ///
    /// Matches CPython 3.14 behavior:
    /// - Raises ``RuntimeError`` when ``~user`` expansion fails (user not found).
    /// - On POSIX, inserts ``./`` before path segments containing a colon to
    ///   avoid ambiguity with Windows drive letters.
    fn expanduser<'py>(slf: PyRef<'py, Self>) -> PyResult<PyObject> {
        let py = slf.py();
        let raw_str = slf.inner.raw().to_string_lossy();

        if !raw_str.starts_with('~') {
            return Self::_make_child(py, slf.as_ptr(), OsString::from(raw_str.as_ref()));
        }

        // Extract the tilde part (~ or ~username) up to the first /
        let slash_pos = raw_str.find('/');
        let (tilde_name, rest) = if let Some(pos) = slash_pos {
            (&raw_str[..pos], &raw_str[pos + 1..])
        } else {
            (raw_str.as_ref(), "")
        };

        // Expand the tilde part with os.path.expanduser
        let os_path = py.import("os.path")?;
        let home = os_path.call_method1("expanduser", (tilde_name,))?;
        let home_str: String = home.extract()?;

        // If os.path.expanduser returns the same string, the user was not found
        if home_str == tilde_name {
            return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                "Could not determine home directory for '{raw_str}'"
            )));
        }

        // Build the result path
        let result = if rest.is_empty() {
            // Just the home directory (e.g., ~ → /home/user)
            home_str
        } else {
            // Prepend "./" to avoid confusion with Windows drive letters.
            // e.g., ~/a:b → /home/user/./a:b
            // Applied on all platforms (including Windows) for consistency.
            let tail = if rest.contains(':') {
                format!("./{rest}")
            } else {
                rest.to_string()
            };
            format!("{home_str}/{tail}")
        };

        Self::_make_child(py, slf.as_ptr(), OsString::from(&result))
    }

    /// Return True if the path is absolute.
    ///
    /// On Windows, a path is absolute if it has both a drive and a root
    /// (e.g. ``c:\\\\foo``), or if it is a UNC path starting with two
    /// slashes (e.g. ``\\\\server\\\\share``). A root-only path like
    /// ``\\\\foo`` without a drive is NOT absolute on Windows.
    fn is_absolute(&self) -> bool {
        let p = self.inner.parsed(self.flavour);
        if self._is_windows() {
            // UNC paths (drive starts with \\) are always absolute
            let is_unc = p
                .drive
                .as_ref()
                .is_some_and(|d| d.as_encoded_bytes().starts_with(b"\\\\"));
            is_unc || (p.root.is_some() && p.drive.is_some())
        } else {
            p.root.is_some()
        }
    }

    /// Return a cached ``PathInfo`` object for this path (CPython 3.12+).
    ///
    /// ``PathInfo`` caches stat results so repeated calls to
    /// ``info.exists()``, ``info.is_dir()``, etc. do not re-stat the file.
    #[getter]
    fn info<'py>(slf: PyRef<'py, Self>) -> PyResult<PyObject> {
        let py = slf.py();
        // Check if we already have a cached PathInfo
        {
            let guard = slf.path_info.lock().unwrap();
            if let Some(ref info) = *guard {
                return Ok(info.clone_ref(py).into_pyobject(py)?.into_any().unbind());
            }
        }
        // Create a new PathInfo and cache it
        let raw_str = slf.inner.raw().to_string_lossy().into_owned();
        let info = Py::new(py, PathInfo::new(&raw_str))?;
        let mut guard = slf.path_info.lock().unwrap();
        *guard = Some(info.clone_ref(py));
        Ok(info.into_pyobject(py)?.into_any().unbind())
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
        let self_parsed = self.inner.parsed(self.flavour);
        if self._is_windows() {
            Ok(self_parsed.eq_windows(&other_parsed))
        } else {
            Ok(self_parsed == &other_parsed)
        }
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
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), self.flavour);
        let self_key = _cmp_key(self.inner.parsed(self.flavour), self._is_windows());
        let other_key = _cmp_key(&other_parsed, self._is_windows());
        Ok(self_key < other_key)
    }

    fn __le__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), self.flavour);
        let self_key = _cmp_key(self.inner.parsed(self.flavour), self._is_windows());
        let other_key = _cmp_key(&other_parsed, self._is_windows());
        Ok(self_key <= other_key)
    }

    fn __gt__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), self.flavour);
        let self_key = _cmp_key(self.inner.parsed(self.flavour), self._is_windows());
        let other_key = _cmp_key(&other_parsed, self._is_windows());
        Ok(self_key > other_key)
    }

    fn __ge__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), self.flavour);
        let self_key = _cmp_key(self.inner.parsed(self.flavour), self._is_windows());
        let other_key = _cmp_key(&other_parsed, self._is_windows());
        Ok(self_key >= other_key)
    }

    fn __str__(&self) -> String {
        let raw = self.inner.raw().to_string_lossy().into_owned();
        if self._is_windows() {
            raw.replace('/', "\\")
        } else {
            raw
        }
    }

    fn __repr__(&self) -> String {
        let class_name = match self.flavour {
            PathFlavour::Posix => "PurePosixPath",
            PathFlavour::Windows => "PureWindowsPath",
        };
        format!("{}('{}')", class_name, self._str_repr())
    }

    fn __fspath__(&self) -> String {
        self.__str__()
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

#[pyclass(subclass, extends=PurePath, module = "pathlibrs")]
pub struct PurePosixPath;

#[pymethods]
impl PurePosixPath {
    #[new]
    #[pyo3(signature = (*args))]
    fn new(args: &Bound<'_, PyTuple>) -> PyResult<(Self, PurePath)> {
        let raw = join_path_segments(args, PathFlavour::Posix)?;
        Ok((Self, PurePath::new_posix(raw)))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PureWindowsPath
// ═══════════════════════════════════════════════════════════════════════

#[pyclass(subclass, extends=PurePath, module = "pathlibrs")]
pub struct PureWindowsPath;

#[pymethods]
impl PureWindowsPath {
    #[new]
    #[pyo3(signature = (*args))]
    fn new(args: &Bound<'_, PyTuple>) -> PyResult<(Self, PurePath)> {
        let raw = join_path_segments(args, PathFlavour::Windows)?;
        Ok((Self, PurePath::new_windows(raw)))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// helpers
// ═══════════════════════════════════════════════════════════════════════

/// Join path segments into a single raw path string.
///
/// Follows CPython's behaviour: when a segment is anchored (has a drive or root),
/// all previously accumulated segments are discarded and the path restarts from
/// that anchored segment.
fn join_path_segments(args: &Bound<'_, PyTuple>, flavour: PathFlavour) -> PyResult<OsString> {
    // A single empty arg ("") produces an empty path, matching CPython.
    if args.len() == 1 {
        if let Ok(first) = args.get_item(0) {
            let s = _extract_path_str(&first)?;
            if s.is_empty() {
                return Ok(OsString::from(""));
            }
        }
    }

    let sep = if flavour == PathFlavour::Windows {
        b'\\'
    } else {
        b'/'
    };
    let mut drive: Option<OsString> = None;
    let mut root: Option<OsString> = None;
    let mut parts: Vec<OsString> = Vec::new();

    for arg in args.iter() {
        let s = _extract_path_str(&arg)?;
        if s.is_empty() {
            continue;
        }
        let parsed = crate::parsing::parse_path(OsStr::new(&s), flavour);
        if parsed.drive.is_some() || parsed.root.is_some() {
            // Anchored segment — reset the accumulated path.
            // When the new segment has a drive it replaces the old one;
            // when it has a root it replaces the root.
            // Only when both are present does the drive reset to None.
            if parsed.drive.is_some() {
                drive = parsed.drive;
            }
            if parsed.root.is_some() {
                root = parsed.root;
            }
            parts = parsed.parts;
        } else {
            // Relative segment — append its parts
            parts.extend(parsed.parts);
        }
    }

    let mut result = Vec::<u8>::new();
    if let Some(ref d) = drive {
        result.extend_from_slice(d.as_encoded_bytes());
    }
    if let Some(ref r) = root {
        result.extend_from_slice(r.as_encoded_bytes());
    }
    for (i, part) in parts.iter().enumerate() {
        if i > 0 {
            result.push(sep);
        }
        result.extend_from_slice(part.as_encoded_bytes());
    }

    if result.is_empty() {
        Ok(OsString::from("."))
    } else {
        Ok(crate::from_os_bytes(&result).to_os_string())
    }
}

/// Build a comparison-tuple key from a parsed path.
///
/// On Windows, drive and parts are lower-cased for case-insensitive ordering.
fn _cmp_key(parsed: &crate::repr::ParsedPath, windows: bool) -> (String, String, Vec<String>) {
    let drive_key = parsed
        .drive
        .as_ref()
        .map(|d| {
            let s = d.to_string_lossy().into_owned();
            if windows {
                s.to_ascii_lowercase()
            } else {
                s
            }
        })
        .unwrap_or_default();
    let root_key = parsed
        .root
        .as_ref()
        .map(|r| r.to_string_lossy().into_owned())
        .unwrap_or_default();
    let parts_key: Vec<String> = parsed
        .parts
        .iter()
        .map(|part| {
            let s = part.to_string_lossy().into_owned();
            if windows {
                s.to_ascii_lowercase()
            } else {
                s
            }
        })
        .collect();
    (drive_key, root_key, parts_key)
}

/// Split a drive-like prefix from a glob pattern string.
///
/// Returns ``(drive, rest)`` for Windows drive prefixed patterns like
/// ``"*:/*.py"`` or ``"c:/*.py"``.
fn _split_drive_from_pattern(pattern: &str) -> Option<(&str, bool, &str)> {
    let bytes = pattern.as_bytes();
    let colon_pos = bytes.iter().position(|&b| b == b':')?;
    if colon_pos == 0 {
        return None;
    }
    let is_drive_like = bytes[..colon_pos]
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'*' || b == b'?' || b == b'[');
    if !is_drive_like {
        return None;
    }
    let after_colon = &pattern[colon_pos + 1..];
    let has_root = after_colon.starts_with('/') || after_colon.starts_with('\\');
    let rest = after_colon
        .strip_prefix('/')
        .or_else(|| after_colon.strip_prefix('\\'))
        .unwrap_or(after_colon);
    let drive = &pattern[..=colon_pos];
    Some((drive, has_root, rest))
}

/// Split the Windows drive prefix from a raw path string.
///
/// Returns ``(drive, rest)`` for paths like ``"c:/foo"`` or UNC
/// ``"\\\\server\\share\\foo"``.
fn _split_drive_from_path(path: &str) -> Option<(&str, bool, &str)> {
    let bytes = path.as_bytes();
    // Drive letter: C: or c:
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let after_colon = &path[2..];
        let has_root = after_colon.starts_with('/') || after_colon.starts_with('\\');
        let rest = after_colon
            .strip_prefix('/')
            .or_else(|| after_colon.strip_prefix('\\'))
            .unwrap_or(after_colon);
        return Some((&path[..2], has_root, rest));
    }
    // UNC: \\server\share
    if bytes.len() > 2 && bytes[0] == b'\\' && bytes[1] == b'\\' {
        let after = &bytes[2..];
        if let Some(sep1) = after.iter().position(|&b| b == b'\\' || b == b'/') {
            let after_server = &after[sep1 + 1..];
            if let Some(sep2) = after_server.iter().position(|&b| b == b'\\' || b == b'/') {
                let drive_end = 2 + sep1 + 1 + sep2;
                let rest = &path[(drive_end + 1).min(path.len())..];
                return Some((&path[..drive_end], true, rest));
            }
        }
    }
    None
}

/// Compare two drive components for equality.
///
/// On Windows, drive comparison is case-insensitive (e.g. ``"C:"`` == ``"c:"``).
fn _drives_equal(a: &Option<OsString>, b: &Option<OsString>, windows: bool) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => {
            if windows {
                a.as_encoded_bytes()
                    .eq_ignore_ascii_case(b.as_encoded_bytes())
            } else {
                a == b
            }
        }
        (None, None) => true,
        _ => false,
    }
}

/// Extract a string from a Python object.
fn _extract_path_str(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    // First try str extraction (only works for str and str subclasses)
    if let Ok(s) = obj.extract::<String>() {
        return Ok(s);
    }
    // PathLike (has __fspath__)
    if let Ok(fspath) = obj.call_method0("__fspath__") {
        return fspath.extract::<String>();
    }
    // Fallback to str() conversion for compatibility
    Ok(obj.str()?.to_string())
}

/// Parse a ``file:`` URI into a path string.
///
/// Supports:
/// - ``file:///absolute/path`` (POSIX)
/// - ``file:relative/path`` (POSIX)
/// - ``file:///C:/path`` (Windows drive letter)
/// - ``file://host/path`` (non-localhost host → error)
fn parse_file_uri(uri: &str) -> PyResult<String> {
    // Strip the "file:" prefix
    let rest = uri
        .strip_prefix("file:")
        .or_else(|| uri.strip_prefix("FILE:"))
        .ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("URI '{uri}' is not a file: URI"))
        })?;

    // Check for authority (//)
    let authority_rest = match rest.strip_prefix("//") {
        Some(ar) => ar,
        None => {
            // file:relative/path → relative path
            return Ok(rest.to_string());
        }
    };

    // Find the first / after the authority
    let (authority, path_part) = match authority_rest.find('/') {
        Some(idx) => {
            let (auth, path) = authority_rest.split_at(idx);
            (auth, &path[1..]) // skip the /
        }
        None => {
            // file://hostname → no path
            (authority_rest, "")
        }
    };

    // If authority is empty or "localhost", it's a local path
    if authority.is_empty() || authority.eq_ignore_ascii_case("localhost") {
        if path_part.is_empty() {
            return Ok("/".to_string());
        }

        // Windows drive letter: /C:/path or /C|/path
        if path_part.len() >= 3
            && path_part.as_bytes()[0].is_ascii_alphabetic()
            && (path_part.as_bytes()[1] == b':' || path_part.as_bytes()[1] == b'|')
            && path_part.as_bytes()[2] == b'/'
        {
            let drive = path_part.as_bytes()[0] as char;
            let rest_path = &path_part[3..];
            if rest_path.is_empty() {
                Ok(format!("{drive}:\\"))
            } else {
                Ok(format!("{drive}:\\{rest_path}"))
            }
        } else {
            Ok(format!("/{path_part}"))
        }
    } else {
        // Non-local authority — not a local path
        Err(pyo3::exceptions::PyValueError::new_err(format!(
            "non-local file: URI not supported: '{uri}'"
        )))
    }
}
