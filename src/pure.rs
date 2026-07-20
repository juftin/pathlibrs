//! PyO3 classes: ``PurePath``, ``PurePosixPath``, ``PureWindowsPath``.
//!
//! Implements all Phase 1 properties and methods matching CPython 3.12+ pathlib.

use std::ffi::{OsStr, OsString};
use std::hash::{Hash, Hasher};
use std::path::Path as StdPath;
use std::sync::Mutex;

use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict, PyList, PyString, PyTuple, PyType};

use crate::fs::PathInfo;
use crate::iter::{GlobIter, ParentsIter};
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
    ///
    /// Uses ``with_segments`` so that subclasses that override it to carry extra
    /// state (e.g., ``session_id``) get their state preserved. This matches
    /// CPython's ``_make_child`` which delegates to ``self.with_segments``.
    fn _make_child(
        py: Python<'_>,
        slf_ptr: *mut pyo3::ffi::PyObject,
        new_raw: OsString,
    ) -> PyResult<PyObject> {
        let slf_bound = unsafe { pyo3::Bound::<'_, pyo3::PyAny>::from_borrowed_ptr(py, slf_ptr) };
        // Call slf.with_segments(new_raw) rather than cls(new_raw) so that
        // subclasses that override with_segments preserve extra state.
        let raw_str = new_raw.to_string_lossy().into_owned();
        Ok(slf_bound
            .call_method1("with_segments", (raw_str,))?
            .unbind())
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
    #[pyo3(signature = (*args, **kwargs))]
    fn new(args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let _ = kwargs;
        #[cfg(windows)]
        let join_flavour = PathFlavour::Windows;
        #[cfg(not(windows))]
        let join_flavour = PathFlavour::Posix;
        let raw = join_path_segments(args, join_flavour)?;
        // Normalize empty path to "." matching CPython (affects __eq__ comparisons).
        let raw = if raw.as_encoded_bytes().is_empty() {
            OsString::from(".")
        } else {
            raw
        };
        Ok(Self {
            inner: PathRepr::new(raw),
            #[cfg(windows)]
            flavour: PathFlavour::Windows,
            #[cfg(not(windows))]
            flavour: PathFlavour::Posix,
            path_info: Mutex::new(None),
        })
    }

    /// No-op initialiser (CPython compat).
    ///
    /// Prevents fall-through to ``object.__init__()`` when subclasses call
    /// ``super().__init__(*pathsegments)``.
    #[pyo3(signature = (*args))]
    fn __init__(&self, args: &Bound<'_, PyTuple>) -> PyResult<()> {
        let _ = args;
        Ok(())
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

    /// The concatenation of the drive and root, or ''.
    #[getter]
    fn anchor(&self) -> String {
        self._anchor_str()
    }

    /// Internal: return the name, or `None` when there is no name.
    fn _name_option(&self) -> Option<String> {
        let p = self.inner.parsed(self.flavour);
        if !p.has_name {
            return None;
        }
        p.parts.last().map(|s| s.to_string_lossy().into_owned())
    }

    /// The final path component, if any.
    #[getter]
    fn name(&self) -> String {
        self._name_option().unwrap_or_default()
    }

    ///
    /// The final component's last suffix, if any.
    ///
    /// This includes the leading period. For example: '.txt'
    ///
    #[getter]
    fn suffix(&self) -> String {
        match self._name_option() {
            Some(ref n) => suffix_from_name(OsStr::new(n))
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            None => String::new(),
        }
    }

    ///
    /// A list of the final component's suffixes, if any.
    ///
    /// These include the leading periods. For example: ['.tar', '.gz']
    ///
    #[getter]
    fn suffixes(&self) -> Vec<String> {
        match self._name_option() {
            Some(ref n) => ops::suffixes_from_name(OsStr::new(n))
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect(),
            None => Vec::new(),
        }
    }

    /// The final path component, minus its last suffix.
    #[getter]
    fn stem(&self) -> String {
        match self._name_option() {
            Some(ref n) => stem_from_name(OsStr::new(n))
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            None => String::new(),
        }
    }

    /// The logical parent of the path.
    #[getter]
    fn parent<'py>(slf: PyRef<'py, Self>) -> PyResult<PyObject> {
        let py = slf.py();
        let ptr = slf.as_ptr();
        let parent_raw = slf._parent_raw();
        PurePath::_make_child(py, ptr, parent_raw)
    }

    /// A sequence of this path's logical parents.
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

    /// An object providing sequence-like access to the
    /// components in the filesystem path.
    #[getter]
    fn parts<'py>(slf: PyRef<'py, Self>, py: Python<'py>) -> PyResult<PyObject> {
        let p = slf.inner.parsed(slf.flavour);
        // CPython PurePath.parts: (drive + root) as the first element
        // when an anchor is present, then the normalized path parts.
        let mut items: Vec<PyObject> = Vec::with_capacity(p.parts.len() + 1);
        let drive = p
            .drive
            .as_ref()
            .map(|s| s.as_encoded_bytes())
            .unwrap_or(b"");
        let root = p.root.as_ref().map(|s| s.as_encoded_bytes()).unwrap_or(b"");
        if !drive.is_empty() || !root.is_empty() {
            // Combine drive + root into a single anchor part.
            let mut anchor = Vec::with_capacity(drive.len() + root.len());
            anchor.extend_from_slice(drive);
            anchor.extend_from_slice(root);
            items.push(
                crate::from_os_bytes(&anchor)
                    .to_os_string()
                    .into_pyobject(py)?
                    .into(),
            );
        }
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

    /// Combine this path with one or several arguments, and return a
    /// new path representing either a subpath (if all arguments are relative
    /// paths) or a totally different path (if one of the arguments is
    /// anchored).
    ///
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

    /// Return a new path with the file name changed.
    fn with_name<'py>(slf: PyRef<'py, Self>, name: &str) -> PyResult<PyObject> {
        if slf._name_option().is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "'{}' has an empty name",
                slf._str_repr()
            )));
        }
        // Reject empty and reserved names.
        if name.is_empty() || name == "." || name == ".." {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "Invalid name '{name}'"
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

    /// Return a new path with the stem changed.
    fn with_stem<'py>(slf: PyRef<'py, Self>, stem: &str) -> PyResult<PyObject> {
        if slf._name_option().is_none() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "'{}' has an empty name",
                slf._str_repr()
            )));
        }
        let name = slf._name_option().unwrap_or_default();
        let old_suffix = suffix_from_name(OsStr::new(&name))
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let new_name = format!("{stem}{old_suffix}");
        PurePath::with_name(slf, &new_name)
    }

    /// Return a new path with the file suffix changed.  If the path
    /// has no suffix, add given suffix.  If the given suffix is an empty
    /// string, remove the suffix from the path.
    ///
    fn with_suffix<'py>(slf: PyRef<'py, Self>, suffix: &str) -> PyResult<PyObject> {
        let name = slf._name_option().unwrap_or_default();
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
                .filter(|p| p.as_encoded_bytes() != b".")
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

    /// Construct a new path object from any number of path-like objects.
    /// Subclasses may override this method to customize how new path objects
    /// are created from methods like `iterdir()`.
    ///
    #[classmethod]
    #[pyo3(signature = (*pathsegments))]
    fn with_segments(
        _cls: &Bound<'_, PyType>,
        pathsegments: &Bound<'_, PyTuple>,
    ) -> PyResult<PyObject> {
        let _py = _cls.py();
        let parts: Vec<String> = pathsegments
            .iter()
            .map(|item| _extract_path_str(&item))
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
        let cls_name = _cls
            .getattr("__name__")
            .map(|n| n.extract::<String>().unwrap_or_default())
            .unwrap_or_default();
        let is_windows = cls_name.contains("Windows");
        let path_str = parse_file_uri(uri, is_windows)?;
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

        // When walk_up is True, CPython rejects ".." segments in the *other*
        // path because they cannot be walked (they already point above the
        // anchor).  This is enforced regardless of whether the anchors match.
        if walk_up {
            for part in &other_parsed.parts {
                if part.as_encoded_bytes() == b".." {
                    return Err(pyo3::exceptions::PyValueError::new_err(format!(
                        "'..' segment in '{}' cannot be walked",
                        other_str
                    )));
                }
            }
        }

        // Find how many leading segments match
        let min_len = self_parsed.parts.len().min(other_parsed.parts.len());
        let mut common = 0usize;

        if !_drives_equal(&self_parsed.drive, &other_parsed.drive, slf._is_windows())
            || self_parsed.root != other_parsed.root
        {
            // Anchors differ — no common prefix.
            // With walk_up=True, allow it only when BOTH paths have roots
            // AND the other path has at least one part (so ".." can
            // conceptually reach a common ancestor).
            // walk_up across different anchors only works when both paths
            // have single-letter drives (e.g. C: vs D:), both have roots,
            // and the other path has at least one part.
            let both_regular_drives =
                _is_regular_drive(&self_parsed.drive) && _is_regular_drive(&other_parsed.drive);
            let both_have_roots = self_parsed.root.is_some() && other_parsed.root.is_some();
            if !walk_up || !both_have_roots || !both_regular_drives || other_parsed.parts.is_empty()
            {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "'{}' does not start with '{}'",
                    slf._str_repr(),
                    other_str
                )));
            }
            // With walk_up=True and both having roots, produce ".." segments
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
        // Non-absolute paths cannot be expressed as file URIs (RFC 8089).
        if !self.is_absolute() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "relative path can't be expressed as a file URI",
            ));
        }
        let p = self.inner.parsed(self.flavour);
        // Percent-encode the path portion via Python, preserving the drive
        // letter colon (which must not be encoded in file: URIs).
        Python::with_gil(|py| {
            let quote = py.import("urllib.parse")?.getattr("quote")?;
            match self.flavour {
                PathFlavour::Posix => {
                    let encoded: String = quote.call1((self.as_posix(),))?.extract()?;
                    Ok(format!("file://{encoded}"))
                }
                PathFlavour::Windows => {
                    let drive = p
                        .drive
                        .as_ref()
                        .expect("absolute Windows path must have a drive");
                    let drive_str = drive.to_string_lossy();
                    if drive_str.starts_with("\\\\") {
                        let trimmed = drive_str
                            .replace('\\', "/")
                            .trim_start_matches('/')
                            .to_string();
                        let rest = self.as_posix()[p.anchor_length..]
                            .trim_start_matches('/')
                            .to_string();
                        let encoded: String = quote.call1((&rest,))?.extract()?;
                        Ok(format!("file://{trimmed}/{encoded}"))
                    } else {
                        let drive_letter = drive_str.trim_end_matches(':');
                        let rest = self.as_posix()[p.anchor_length..]
                            .trim_start_matches('/')
                            .to_string();
                        let encoded: String = quote.call1((&rest,))?.extract()?;
                        Ok(format!("file:///{drive_letter}:/{encoded}"))
                    }
                }
            }
        })
    }

    #[pyo3(name = "match")]
    #[pyo3(signature = (pattern, *, case_sensitive = None))]
    fn match_(&self, pattern: &str, case_sensitive: Option<bool>) -> PyResult<bool> {
        // Reject empty patterns (CPython pathlib raises ValueError).
        if pattern.is_empty() || pattern == "." {
            return Err(pyo3::exceptions::PyValueError::new_err("empty pattern"));
        }
        // Empty / root-only path never matches anything (CPython pathlib behaviour).
        if self._name_option().is_none() {
            return Ok(false);
        }
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
                        return Ok(false);
                    }
                    // Match drive with fnmatch, then match the rest
                    if !pattern::fnmatch_bytes(pat_drive.as_bytes(), path_drive.as_bytes(), cs) {
                        return Ok(false);
                    }
                    return Ok(pattern::match_path(
                        OsStr::new(pat_rest),
                        OsStr::new(path_rest),
                        cs,
                        is_windows,
                    ));
                }
            }
        }
        Ok(pattern::match_path(
            OsStr::new(pattern),
            self.inner.raw(),
            cs,
            is_windows,
        ))
    }

    ///
    /// Return True if this path matches the given glob-style pattern. The
    /// pattern is matched against the entire path.
    ///
    #[pyo3(name = "full_match")]
    #[pyo3(signature = (pattern, *, case_sensitive = None))]
    fn full_match_(&self, pattern: &str, case_sensitive: Option<bool>) -> PyResult<bool> {
        // Reject empty patterns (CPython pathlib raises ValueError).
        if pattern.is_empty() || pattern == "." {
            return Err(pyo3::exceptions::PyValueError::new_err("empty pattern"));
        }
        let cs = case_sensitive.unwrap_or(!self._is_windows());
        Ok(pattern::full_match_path(
            OsStr::new(pattern),
            self.inner.raw(),
            cs,
            self._is_windows(),
        ))
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
        if self._is_windows() {
            return Err(unsupported_msg("Path.owner()"));
        }
        crate::fs::owner(self.inner.raw(), follow_symlinks)
    }

    /// Return the group name of the file.
    #[pyo3(signature = (*, follow_symlinks = true))]
    fn group(&self, follow_symlinks: bool) -> PyResult<String> {
        if self._is_windows() {
            return Err(unsupported_msg("Path.group()"));
        }
        crate::fs::group(self.inner.raw(), follow_symlinks)
    }

    /// Resolve the path to an absolute path, resolving symlinks.
    #[pyo3(signature = (strict = false))]
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

    /// Check whether the path is a reserved name (Windows only).
    ///
    /// This method is deprecated as of Python 3.13. It always returns ``False``
    /// on POSIX. On Windows it checks for reserved names (CON, PRN, AUX, NUL,
    /// COM1-COM9, LPT1-LPT9).
    #[pyo3(name = "is_reserved")]
    fn is_reserved_impl(&self) -> PyResult<bool> {
        Python::with_gil(|py| {
            let _ = py.import("warnings")?.call_method1(
                "warn",
                (
                    concat!(
                        "pathlib.PurePath.is_reserved() is deprecated and scheduled for ",
                        "removal in a future Python version. If you use this method, ",
                        "please open a discussion on the CPython issue tracker.",
                    ),
                    py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),
                ),
            );
            Ok::<_, PyErr>(())
        })?;
        if !self._is_windows() {
            return Ok(false);
        }
        // Check Windows reserved names in the last component.
        let name = self._name_option().unwrap_or_default();
        let upper = name.to_uppercase();
        // Check exact reserved names
        let reserved = ["CON", "PRN", "AUX", "NUL", "CONIN$", "CONOUT$"];
        if reserved.contains(&upper.as_str()) {
            return Ok(true);
        }
        // Check COM1-COM9 and LPT1-LPT9
        if upper.len() >= 3 {
            if let Some(suffix) = upper.strip_prefix("COM") {
                if let Ok(n) = suffix.parse::<u32>() {
                    if (1..=9).contains(&n) || suffix == "¹" || suffix == "²" || suffix == "³" {
                        return Ok(true);
                    }
                }
            }
            if let Some(suffix) = upper.strip_prefix("LPT") {
                if let Ok(n) = suffix.parse::<u32>() {
                    if (1..=9).contains(&n) || suffix == "¹" || suffix == "²" || suffix == "³" {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
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
        // Return NotImplemented for non-str / non-PathLike objects (matching CPython).
        let other_str = match _try_extract_path_str(other) {
            Ok(s) => s,
            Err(_) => return Ok(py.NotImplemented()),
        };
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
        // Return NotImplemented for non-str / non-PathLike objects (matching CPython).
        let other_str = match _try_extract_path_str(other) {
            Ok(s) => s,
            Err(_) => return Ok(py.NotImplemented()),
        };
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
        // CPython 3.14+: Only PurePath instances can be compared for equality.
        // For non-PurePath types, __eq__ returns NotImplemented, which causes
        // Python to try the reflected operation and eventually fall back to
        // identity comparison (always False for different types).
        if !other.is_instance_of::<Self>() {
            return Ok(false);
        }
        // Paths with different parsers/flavours are never equal.
        if !_same_flavour(other, self.flavour) {
            return Ok(false);
        }
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), self.flavour);
        let self_parsed = self.inner.parsed(self.flavour);
        if !self._is_windows() {
            return Ok(self_parsed == &other_parsed);
        }
        // Quick structural check first
        if self_parsed.root != other_parsed.root
            || self_parsed.parts.len() != other_parsed.parts.len()
        {
            return Ok(false);
        }
        // Use Python casefold for Unicode-aware case-insensitive comparison.
        // Fall back to ASCII-only comparison when all components are ASCII.
        let needs_unicode = |s: &OsString| s.as_encoded_bytes().iter().any(|&b| b >= 128);
        let any_non_ascii = self_parsed.drive.as_ref().is_some_and(needs_unicode)
            || other_parsed.drive.as_ref().is_some_and(needs_unicode)
            || self_parsed.parts.iter().any(&needs_unicode)
            || other_parsed.parts.iter().any(needs_unicode);

        if any_non_ascii {
            Python::with_gil(|py| {
                let drive_eq = match (&self_parsed.drive, &other_parsed.drive) {
                    (Some(a), Some(b)) => {
                        let a_py = PyString::new(py, &a.to_string_lossy());
                        let b_py = PyString::new(py, &b.to_string_lossy());
                        a_py.call_method0("casefold")?.extract::<String>()?
                            == b_py.call_method0("casefold")?.extract::<String>()?
                    }
                    (None, None) => true,
                    _ => false,
                };
                if !drive_eq {
                    return Ok(false);
                }
                for (a, b) in self_parsed.parts.iter().zip(other_parsed.parts.iter()) {
                    let a_py = PyString::new(py, &a.to_string_lossy());
                    let b_py = PyString::new(py, &b.to_string_lossy());
                    if a_py.call_method0("casefold")?.extract::<String>()?
                        != b_py.call_method0("casefold")?.extract::<String>()?
                    {
                        return Ok(false);
                    }
                }
                Ok(true)
            })
        } else {
            Ok(self_parsed.eq_windows(&other_parsed))
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
        if !_is_path_instance(other) {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "'<' not supported between instances of 'pathlibrs.PurePath' and '...'",
            ));
        }
        if !_same_flavour(other, self.flavour) {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "'<' not supported between instances of 'PurePosixPath' and 'PureWindowsPath'",
            ));
        }
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), self.flavour);
        let self_key = _cmp_key(self.inner.parsed(self.flavour), self._is_windows());
        let other_key = _cmp_key(&other_parsed, self._is_windows());
        Ok(self_key < other_key)
    }

    fn __le__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        if !_is_path_instance(other) {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "'<=' not supported between instances of 'pathlibrs.PurePath' and '...'",
            ));
        }
        if !_same_flavour(other, self.flavour) {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "'<=' not supported between instances of 'PurePosixPath' and 'PureWindowsPath'",
            ));
        }
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), self.flavour);
        let self_key = _cmp_key(self.inner.parsed(self.flavour), self._is_windows());
        let other_key = _cmp_key(&other_parsed, self._is_windows());
        Ok(self_key <= other_key)
    }

    fn __gt__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        if !_is_path_instance(other) {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "'>' not supported between instances of 'pathlibrs.PurePath' and '...'",
            ));
        }
        if !_same_flavour(other, self.flavour) {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "'>' not supported between instances of 'PurePosixPath' and 'PureWindowsPath'",
            ));
        }
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), self.flavour);
        let self_key = _cmp_key(self.inner.parsed(self.flavour), self._is_windows());
        let other_key = _cmp_key(&other_parsed, self._is_windows());
        Ok(self_key > other_key)
    }

    fn __ge__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        if !_is_path_instance(other) {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "'>=' not supported between instances of 'pathlibrs.PurePath' and '...'",
            ));
        }
        if !_same_flavour(other, self.flavour) {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "'>=' not supported between instances of 'PurePosixPath' and 'PureWindowsPath'",
            ));
        }
        let other_str = _extract_path_str(other)?;
        let other_parsed = crate::parsing::parse_path(OsStr::new(&other_str), self.flavour);
        let self_key = _cmp_key(self.inner.parsed(self.flavour), self._is_windows());
        let other_key = _cmp_key(&other_parsed, self._is_windows());
        Ok(self_key >= other_key)
    }

    fn __str__(&self) -> String {
        let raw = self.inner.raw().to_string_lossy().into_owned();
        if raw.is_empty() {
            // Empty path points to current directory, same as '.'.
            return ".".to_string();
        }
        if self._is_windows() {
            raw.replace('/', "\\")
        } else {
            raw
        }
    }

    fn __repr__<'py>(slf: PyRef<'py, Self>) -> PyResult<String> {
        let py = slf.py();
        let bound = unsafe { pyo3::Bound::<'_, pyo3::PyAny>::from_borrowed_ptr(py, slf.as_ptr()) };
        let cls = bound.getattr("__class__")?;
        let class_name: String = cls.getattr("__name__")?.extract()?;
        // Use as_posix() for the inner repr string (CPython behaviour).
        let inner = slf.as_posix();
        let inner = if inner.is_empty() { "." } else { &inner };
        Ok(format!("{}('{}')", class_name, inner))
    }

    fn __fspath__(&self) -> String {
        self.__str__()
    }

    fn __bytes__(&self) -> PyResult<PyObject> {
        // Use os.fsencode(str(self)) — CPython behaviour.
        // __str__ normalises separators to OS-native form (\ on Windows).
        let raw = self.__str__();
        Python::with_gil(|py| Ok(pyo3::types::PyBytes::new(py, raw.as_bytes()).into()))
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

    // -- Phase 3: Directory mutations -----------------------------------

    ///
    /// Create a new directory at this given path.
    ///
    #[pyo3(signature = (mode = 0o777, parents = false, exist_ok = false))]
    fn mkdir(&self, mode: u32, parents: bool, exist_ok: bool) -> PyResult<()> {
        crate::fs::mkdir(self.inner.raw(), mode, parents, exist_ok)
    }

    /// Remove this empty directory.
    fn rmdir(&self) -> PyResult<()> {
        crate::fs::rmdir(self.inner.raw())
    }

    /// Change file mode (permissions).
    #[pyo3(signature = (mode, *, follow_symlinks = true))]
    fn chmod(&self, mode: u32, follow_symlinks: bool) -> PyResult<()> {
        crate::fs::chmod(self.inner.raw(), mode, follow_symlinks)
    }

    /// Change file mode without following symlinks.
    fn lchmod(&self, mode: u32) -> PyResult<()> {
        crate::fs::chmod(self.inner.raw(), mode, false)
    }

    // -- Phase 3: File mutations ----------------------------------------

    /// Create a file or update its modification time.
    #[pyo3(signature = (mode = 0o666, exist_ok = true))]
    fn touch(&self, mode: u32, exist_ok: bool) -> PyResult<()> {
        crate::fs::touch(self.inner.raw(), mode, exist_ok)
    }

    /// Remove (unlink) this file or symlink.
    #[pyo3(signature = (missing_ok = false))]
    fn unlink(&self, missing_ok: bool) -> PyResult<()> {
        crate::fs::unlink(self.inner.raw(), missing_ok)
    }

    /// Rename this file or directory to the given target.
    fn rename<'py>(slf: PyRef<'py, Self>, target: &Bound<'py, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let target_str = _extract_path_str(target)?;
        crate::fs::rename(slf.inner.raw(), OsStr::new(&target_str))?;
        Self::_make_child(py, slf.as_ptr(), OsString::from(&target_str))
    }

    /// Replace this file or directory with the given target.
    fn replace<'py>(slf: PyRef<'py, Self>, target: &Bound<'py, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let target_str = _extract_path_str(target)?;
        crate::fs::replace(slf.inner.raw(), OsStr::new(&target_str))?;
        Self::_make_child(py, slf.as_ptr(), OsString::from(&target_str))
    }

    // -- Phase 3: Link creation -----------------------------------------

    ///
    /// Make this path a symlink pointing to the target path.
    /// Note the order of arguments (link, target) is the reverse of os.symlink.
    ///
    #[pyo3(signature = (target, target_is_directory = false))]
    fn symlink_to(&self, target: &Bound<'_, PyAny>, target_is_directory: bool) -> PyResult<()> {
        let target_str = _extract_path_str(target)?;
        crate::fs::symlink(
            OsStr::new(&target_str),
            self.inner.raw(),
            target_is_directory,
        )
    }

    /// Create a hard link at this path pointing to *target*.
    ///
    /// In CPython, ``self.hardlink_to(target)`` is equivalent to
    /// ``os.link(target, self)`` — i.e., *self* is the new link name,
    /// *target* is the existing file.
    fn hardlink_to(&self, target: &Bound<'_, PyAny>) -> PyResult<()> {
        let target_str = _extract_path_str(target)?;
        crate::fs::hardlink(OsStr::new(&target_str), self.inner.raw())
    }

    // -- Phase 3: File I/O ----------------------------------------------

    /// Open this file.
    ///
    /// Delegates to Python's ``io.open()`` per DESIGN.md §11.1 for full
    /// compatibility with all open() parameters.
    #[pyo3(signature = (mode = "r", buffering = -1, encoding = None, errors = None, newline = None))]
    fn open<'py>(
        slf: PyRef<'py, Self>,
        mode: &str,
        buffering: isize,
        encoding: Option<&str>,
        errors: Option<&str>,
        newline: Option<&str>,
    ) -> PyResult<PyObject> {
        let py = slf.py();
        let io_mod = py.import("io")?;
        let path_str = slf.inner.raw().to_string_lossy().into_owned();
        let kwargs = pyo3::types::PyDict::new(py);
        kwargs.set_item("mode", mode)?;
        kwargs.set_item("buffering", buffering)?;
        if let Some(enc) = encoding {
            kwargs.set_item("encoding", enc)?;
        }
        if let Some(err) = errors {
            kwargs.set_item("errors", err)?;
        }
        if let Some(nl) = newline {
            kwargs.set_item("newline", nl)?;
        }
        Ok(io_mod
            .call_method("open", (path_str,), Some(&kwargs))?
            .unbind())
    }

    /// Read the entire file as bytes.
    fn read_bytes(&self) -> PyResult<Vec<u8>> {
        crate::fs::read_bytes(self.inner.raw())
    }

    /// Read the entire file as text.
    #[pyo3(signature = (encoding = None, errors = None))]
    fn read_text(&self, encoding: Option<&str>, errors: Option<&str>) -> PyResult<String> {
        crate::fs::read_text(self.inner.raw(), encoding, errors)
    }

    ///
    /// Open the file in bytes mode, write to it, and close the file.
    ///
    fn write_bytes(&self, data: Vec<u8>) -> PyResult<()> {
        crate::fs::write_bytes(self.inner.raw(), &data)
    }

    ///
    /// Open the file in text mode, write to it, and close the file.
    ///
    #[pyo3(signature = (data, encoding = None, errors = None, newline = None))]
    fn write_text(
        &self,
        data: &str,
        encoding: Option<&str>,
        errors: Option<&str>,
        newline: Option<&str>,
    ) -> PyResult<()> {
        crate::fs::write_text(self.inner.raw(), data, encoding, errors, newline)
    }

    // -- Phase 3: Directory traversal -----------------------------------

    /// Iterate over the directory contents as Path objects.
    ///
    /// Returns a list of Path objects representing the directory contents.
    /// Each entry is a full path (dirpath / name), matching CPython behavior.
    fn iterdir<'py>(slf: PyRef<'py, Self>) -> PyResult<PyObject> {
        let py = slf.py();
        let ptr = slf.as_ptr();
        let entries = crate::fs::read_dir(slf.inner.raw())?;
        let mut paths: Vec<PyObject> = Vec::with_capacity(entries.len());
        for entry in &entries {
            let child = Self::_make_child(py, ptr, entry.path.clone())?;
            paths.push(child);
        }
        Ok(PyList::new(py, paths)?.into_any().unbind())
    }

    /// Walk a directory tree recursively.
    ///
    /// Yields ``(dirpath, dirnames, filenames)`` tuples. The caller may
    /// modify ``dirnames`` in-place to control which subdirectories are
    /// visited next (when ``topdown=True``).
    #[pyo3(signature = (top_down = true, on_error = None, follow_symlinks = false))]
    fn walk<'py>(
        slf: PyRef<'py, Self>,
        top_down: bool,
        on_error: Option<PyObject>,
        follow_symlinks: bool,
    ) -> PyResult<PyObject> {
        let py = slf.py();
        let source = unsafe { Py::from_borrowed_ptr(py, slf.as_ptr()) };
        let root = slf.inner.raw().to_os_string();
        let iter = WalkIter::new(source, root, top_down, follow_symlinks, on_error);
        Ok(Py::new(py, iter)?.into_pyobject(py)?.into_any().unbind())
    }

    // -- Phase 4: Glob & Pattern Matching --------------------------------

    /// Iterate over this directory tree, yielding all matching files.
    ///
    /// Parameters
    /// ----------
    /// pattern : str | os.PathLike
    ///     The glob pattern (relative only).
    /// case_sensitive : bool | None
    ///     If ``True``, pattern matching is case-sensitive.
    ///     If ``False``, case-insensitive.
    ///     If ``None`` (default), uses the platform default
    ///     (case-sensitive on POSIX, case-insensitive on Windows).
    /// recurse_symlinks : bool
    ///     If ``True``, follow symlinks to directories (default ``False``).
    #[pyo3(signature = (pattern, *, case_sensitive = None, recurse_symlinks = false))]
    fn glob<'py>(
        slf: PyRef<'py, Self>,
        pattern: &Bound<'py, PyAny>,
        case_sensitive: Option<bool>,
        recurse_symlinks: bool,
    ) -> PyResult<PyObject> {
        let py = slf.py();
        let pattern_str = _extract_path_str(pattern)?;
        let cs = case_sensitive.unwrap_or(!slf._is_windows());

        let opts = crate::glob::GlobOptions {
            case_sensitive: cs,
            recurse_symlinks,
            case_pedantic: case_sensitive.is_some(),
        };

        let base = slf.inner.raw();

        // Collect results as strings
        let results = match crate::glob::glob_walk(base, &pattern_str, &opts) {
            Ok(r) => r,
            Err(msg) => {
                return Err(pyo3::exceptions::PyValueError::new_err(msg));
            }
        };

        // Convert OsStrings to normalized strings.
        // Strip "./" prefix when base is "." (CPython _remove_leading_dot).
        let base_str = base.to_string_lossy();
        let strip_dot = base_str.as_ref() == "." || base_str.as_ref() == "./";
        let str_results: Vec<String> = results
            .iter()
            .map(|p| {
                let mut s = p.to_string_lossy().into_owned();
                if strip_dot {
                    // Strip "./" or ".\\" prefix from results
                    if let Some(rest) = s.strip_prefix("./").or_else(|| s.strip_prefix(".\\")) {
                        s = rest.to_string();
                    }
                }
                s
            })
            .collect();

        let source: PyObject = slf.into_pyobject(py)?.into_any().unbind();
        let iter = GlobIter::new(str_results, source);
        Ok(Py::new(py, iter)?.into_pyobject(py)?.into_any().unbind())
    }

    /// Recursive glob: like ``glob()`` but with ``**/`` prepended to the pattern.
    ///
    /// Parameters match ``glob()``.
    #[pyo3(signature = (pattern, *, case_sensitive = None, recurse_symlinks = false))]
    fn rglob<'py>(
        slf: PyRef<'py, Self>,
        pattern: &Bound<'py, PyAny>,
        case_sensitive: Option<bool>,
        recurse_symlinks: bool,
    ) -> PyResult<PyObject> {
        let pattern_str = _extract_path_str(pattern)?;
        // CPython: rglob(pattern) = glob(self.parser.join('**', pattern))
        let recursive_pattern = if pattern_str.is_empty() {
            "**/".to_string()
        } else {
            format!("**/{pattern_str}")
        };
        let py = slf.py();
        let py_pattern = pyo3::types::PyString::new(py, &recursive_pattern);
        Self::glob(
            slf,
            &py_pattern.into_any(),
            case_sensitive,
            recurse_symlinks,
        )
    }

    // -- Phase 3: 3.14 file-tree operations -----------------------------

    /// Copy this file or directory tree to *target*.
    ///
    /// If *target* is an existing directory, the source is copied *into* it
    /// (as ``target / source.name``).  CPython copies to the *exact* target
    /// path — only ``copy_into`` appends ``source.name``.
    #[pyo3(signature = (target, *, follow_symlinks = true, dirs_exist_ok = false,
        preserve_metadata = false, ignore = None, on_error = None))]
    fn copy<'py>(
        slf: PyRef<'py, Self>,
        target: &Bound<'py, PyAny>,
        follow_symlinks: bool,
        dirs_exist_ok: bool,
        preserve_metadata: bool,
        ignore: Option<PyObject>,
        on_error: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let py = slf.py();
        let target_str = _extract_path_str(target)?;
        let src_str = slf._str_repr();

        // ensure_distinct_paths: raise if source == target or source
        // is a parent of target (CPython pathlib._os.ensure_distinct_paths).
        if src_str == target_str {
            return Err(pyo3::exceptions::PyOSError::new_err(format!(
                "Source and target are the same path: '{}'",
                src_str
            )));
        }
        // Check if source is a lexical parent of target.
        {
            let src_path = std::path::Path::new(&src_str);
            let dst_path = std::path::Path::new(&target_str);
            if dst_path.starts_with(src_path) {
                // Walk up to see if any component past src_path is '..'
                let rel = dst_path
                    .strip_prefix(src_path)
                    .unwrap_or(std::path::Path::new(""));
                if !rel
                    .components()
                    .any(|c| c == std::path::Component::ParentDir)
                {
                    return Err(pyo3::exceptions::PyOSError::new_err(format!(
                        "Source path is a parent of target path: '{}' -> '{}'",
                        src_str, target_str
                    )));
                }
            }
        }

        let _ = (ignore, on_error);
        crate::fs::copy_tree(
            slf.inner.raw(),
            OsStr::new(&target_str),
            follow_symlinks,
            dirs_exist_ok,
            preserve_metadata,
        )?;

        Self::_make_child(py, slf.as_ptr(), OsString::from(&target_str))
    }

    /// Copy this file or directory tree *into* an existing directory.
    #[pyo3(signature = (target_dir, *, follow_symlinks = true, dirs_exist_ok = false,
        preserve_metadata = false, ignore = None, on_error = None))]
    fn copy_into<'py>(
        slf: PyRef<'py, Self>,
        target_dir: &Bound<'py, PyAny>,
        follow_symlinks: bool,
        dirs_exist_ok: bool,
        preserve_metadata: bool,
        ignore: Option<PyObject>,
        on_error: Option<PyObject>,
    ) -> PyResult<PyObject> {
        let py = slf.py();
        let target_str = _extract_path_str(target_dir)?;
        let name = slf._name_option().unwrap_or_default();
        if name.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "'{}' has an empty name",
                slf._str_repr()
            )));
        }
        let final_dst = format!("{}/{}", target_str.trim_end_matches('/'), name);
        let _ = (ignore, on_error);
        crate::fs::copy_tree(
            slf.inner.raw(),
            OsStr::new(&final_dst),
            follow_symlinks,
            dirs_exist_ok,
            preserve_metadata,
        )?;
        Self::_make_child(py, slf.as_ptr(), OsString::from(&final_dst))
    }

    /// Move this file or directory tree to *target*.
    #[pyo3(name = "move")]
    #[pyo3(signature = (target))]
    /// CPython: move() first tries ``os.replace()``, falling back to
    /// copy+delete.  CPython copies to the *exact* target path — only
    /// ``move_into`` appends ``source.name``.
    fn move_<'py>(slf: PyRef<'py, Self>, target: &Bound<'py, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let target_str = _extract_path_str(target)?;
        let src_str = slf._str_repr();

        // ensure_distinct_paths: raise if source == target (CPython match).
        if src_str == target_str {
            return Err(pyo3::exceptions::PyOSError::new_err(format!(
                "Source and target are the same path: '{}'",
                src_str
            )));
        }

        crate::fs::move_tree(slf.inner.raw(), OsStr::new(&target_str))?;
        Self::_make_child(py, slf.as_ptr(), OsString::from(&target_str))
    }

    /// Move this file or directory tree *into* an existing directory.
    #[pyo3(signature = (target_dir))]
    fn move_into<'py>(slf: PyRef<'py, Self>, target_dir: &Bound<'py, PyAny>) -> PyResult<PyObject> {
        let py = slf.py();
        let target_str = _extract_path_str(target_dir)?;
        let name = slf._name_option().unwrap_or_default();
        if name.is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "'{}' has an empty name",
                slf._str_repr()
            )));
        }
        let final_dst = format!("{}/{}", target_str.trim_end_matches('/'), name);
        crate::fs::move_tree(slf.inner.raw(), OsStr::new(&final_dst))?;
        Self::_make_child(py, slf.as_ptr(), OsString::from(&final_dst))
    }

    /// Delete this file or directory tree recursively.
    #[pyo3(signature = (*, ignore_errors = false))]
    fn delete(&self, ignore_errors: bool) -> PyResult<()> {
        crate::fs::delete_tree(self.inner.raw(), ignore_errors)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// WalkIter — lazy directory-tree iterator used by Path.walk()
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WalkFrameState {
    Pending,
    Yielded,
}

#[derive(Debug)]
struct WalkFrame {
    path: OsString,
    state: WalkFrameState,
    dirnames: Vec<OsString>,
    filenames: Vec<OsString>,
}

/// Lazy iterator for ``Path.walk()``.
///
/// Mirrors CPython's ``os.walk`` generator: each call to ``__next__``
/// reads a single directory, yields its tuple, and pushes its
/// subdirectories onto the stack. This allows the caller to modify the
/// yielded ``dirnames`` list before subdirectories are visited.
#[pyclass(module = "pathlibrs")]
pub struct WalkIter {
    source: Py<PyAny>,
    topdown: bool,
    follow_symlinks: bool,
    on_error: Option<PyObject>,
    stack: Vec<WalkFrame>,
}

impl WalkIter {
    fn new(
        source: Py<PyAny>,
        root: OsString,
        topdown: bool,
        follow_symlinks: bool,
        on_error: Option<PyObject>,
    ) -> Self {
        Self {
            source,
            topdown,
            follow_symlinks,
            on_error,
            stack: vec![WalkFrame {
                path: root,
                state: WalkFrameState::Pending,
                dirnames: Vec::new(),
                filenames: Vec::new(),
            }],
        }
    }

    fn build_tuple<'py>(
        &self,
        py: Python<'py>,
        path: &OsStr,
        dirnames: &[OsString],
        filenames: &[OsString],
    ) -> PyResult<PyObject> {
        let dp: PyObject = self.source.call_method1(
            py,
            "with_segments",
            (path.to_string_lossy().into_owned(),),
        )?;
        let dn: PyObject = PyList::new(
            py,
            dirnames.iter().map(|n| {
                n.to_string_lossy()
                    .into_owned()
                    .into_pyobject(py)
                    .unwrap()
                    .into_any()
                    .unbind()
            }),
        )?
        .into_any()
        .unbind();
        let fn_: PyObject = PyList::new(
            py,
            filenames.iter().map(|n| {
                n.to_string_lossy()
                    .into_owned()
                    .into_pyobject(py)
                    .unwrap()
                    .into_any()
                    .unbind()
            }),
        )?
        .into_any()
        .unbind();
        Ok(PyTuple::new(py, [dp, dn, fn_])?.into_any().unbind())
    }
}

#[pymethods]
impl WalkIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__<'py>(mut slf: PyRefMut<'py, Self>, py: Python<'py>) -> PyResult<Option<PyObject>> {
        while let Some(mut frame) = slf.stack.pop() {
            match frame.state {
                WalkFrameState::Pending => {
                    let entries = match crate::fs::read_dir(&frame.path) {
                        Ok(e) => e,
                        Err(e) => {
                            // CPython's os.walk swallows directory read errors
                            // by default; call on_error only when provided.
                            if let Some(ref handler) = slf.on_error {
                                handler.call1(py, (e,))?;
                            }
                            continue;
                        }
                    };

                    let mut dirnames: Vec<OsString> = Vec::new();
                    let mut filenames: Vec<OsString> = Vec::new();
                    for entry in entries {
                        let is_directory = entry.is_dir
                            || (entry.is_symlink
                                && slf.follow_symlinks
                                && std::fs::metadata(StdPath::new(&entry.path))
                                    .map(|m| m.is_dir())
                                    .unwrap_or(false));
                        if is_directory {
                            dirnames.push(entry.name);
                        } else {
                            filenames.push(entry.name);
                        }
                    }

                    if slf.topdown {
                        // Yield now; children will be pushed after the caller
                        // has had a chance to modify `dirnames`.
                        let path = frame.path.clone();
                        frame.dirnames = dirnames.clone();
                        frame.filenames = filenames.clone();
                        frame.state = WalkFrameState::Yielded;
                        slf.stack.push(frame);
                        return Ok(Some(slf.build_tuple(py, &path, &dirnames, &filenames)?));
                    }

                    // Bottom-up: push a marker for this directory, then push
                    // children so they are yielded first.
                    let path = frame.path.clone();
                    frame.dirnames = dirnames.clone();
                    frame.filenames = filenames;
                    frame.state = WalkFrameState::Yielded;
                    slf.stack.push(frame);
                    for name in dirnames.iter().rev() {
                        let child_path = StdPath::new(&path).join(name).as_os_str().to_os_string();
                        slf.stack.push(WalkFrame {
                            path: child_path,
                            state: WalkFrameState::Pending,
                            dirnames: Vec::new(),
                            filenames: Vec::new(),
                        });
                    }
                }
                WalkFrameState::Yielded => {
                    if slf.topdown {
                        // Push children (respecting any in-place modifications
                        // to `dirnames` made by the caller).
                        for name in frame.dirnames.iter().rev() {
                            let child_path = StdPath::new(&frame.path)
                                .join(name)
                                .as_os_str()
                                .to_os_string();
                            slf.stack.push(WalkFrame {
                                path: child_path,
                                state: WalkFrameState::Pending,
                                dirnames: Vec::new(),
                                filenames: Vec::new(),
                            });
                        }
                    } else {
                        // Bottom-up marker: all children have been yielded.
                        return Ok(Some(slf.build_tuple(
                            py,
                            &frame.path,
                            &frame.dirnames,
                            &frame.filenames,
                        )?));
                    }
                }
            }
        }
        Ok(None)
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
// PureWindowsPath
// ═══════════════════════════════════════════════════════════════════════

#[pyclass(subclass, extends=PurePath, module = "pathlibrs")]
pub struct PureWindowsPath;

#[pymethods]
impl PureWindowsPath {
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

// ═══════════════════════════════════════════════════════════════════════
// helpers
// ═══════════════════════════════════════════════════════════════════════

/// Join path segments into a single raw path string.
///
/// Follows CPython's behaviour: when a segment is anchored (has a drive or root),
/// all previously accumulated segments are discarded and the path restarts from
/// that anchored segment.
pub(crate) fn join_path_segments(
    args: &Bound<'_, PyTuple>,
    flavour: PathFlavour,
) -> PyResult<OsString> {
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
        // When an arg is a PurePath-like object, convert to the target
        // flavour's string representation before parsing.  This handles
        // cross-flavour cases like PurePosixPath(PureWindowsPath(...)).
        if arg.getattr("parts").is_ok() {
            let path_str: String = if flavour == PathFlavour::Posix {
                arg.call_method0("as_posix")?.extract()?
            } else {
                arg.str()?.to_string()
            };
            if path_str.is_empty() {
                continue;
            }
            let parsed = crate::parsing::parse_path(OsStr::new(&path_str), flavour);
            if parsed.drive.is_some() || parsed.root.is_some() {
                if parsed.drive.is_some() {
                    drive = parsed.drive;
                }
                if parsed.root.is_some() {
                    root = parsed.root;
                }
                parts = parsed.parts;
            } else {
                parts.extend(parsed.parts);
            }
            continue;
        }

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

    // On Windows, a relative path whose first part contains a colon (e.g.
    // "c:a") looks like a drive-relative reference.  Insert a leading ".\"
    // to prevent the reconstructed path from being reparsed as drive-relative.
    // This preserves the intent of inputs like "./c:a" and results of
    // with_name("d:") / with_stem("d:").
    if flavour == PathFlavour::Windows
        && drive.is_none()
        && root.is_none()
        && !result.is_empty()
        && !parts.is_empty()
        && parts[0].as_encoded_bytes().contains(&b':')
    {
        let mut prefixed = Vec::with_capacity(2 + result.len());
        prefixed.extend_from_slice(b".\\");
        prefixed.extend_from_slice(&result);
        return Ok(crate::from_os_bytes(&prefixed).to_os_string());
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

/// Check whether a drive is a single-letter Windows drive (e.g. ``"C:"``).
fn _is_regular_drive(drive: &Option<OsString>) -> bool {
    match drive {
        Some(d) => {
            let bytes = d.as_encoded_bytes();
            bytes.len() == 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
        }
        None => false,
    }
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

/// Check whether `other` is a PurePath instance (or subclass thereof).
///
/// Returns ``true`` for PurePath, PurePosixPath, PureWindowsPath,
/// PosixPath, WindowsPath, and any user-defined subclasses.
/// Uses duck-type check: must have a ``parser`` attribute with a ``sep``.
fn _is_path_instance(other: &Bound<'_, PyAny>) -> bool {
    other
        .getattr("parser")
        .and_then(|p| p.getattr("sep"))
        .is_ok()
}

/// Check whether `other` has the same parser/flavour as `expected_flavour`.
///
/// PurePosixPath and PureWindowsPath have different parsers (posixpath vs ntpath).
/// Paths with different parsers are never equal and cannot be ordered.
fn _same_flavour(other: &Bound<'_, PyAny>, expected_flavour: PathFlavour) -> bool {
    if let Ok(parser) = other.getattr("parser") {
        if let Ok(sep) = parser.getattr("sep") {
            if let Ok(sep_str) = sep.extract::<String>() {
                match expected_flavour {
                    PathFlavour::Posix => return sep_str == "/",
                    PathFlavour::Windows => return sep_str == "\\",
                }
            }
        }
    }
    // If we can't determine the flavour, conservatively treat as same.
    true
}

/// Extract a string from a Python object that is either a str or a PathLike.
///
/// Unlike [`_extract_path_str`], this function returns an error for objects
/// that are not ``str`` and do not implement ``__fspath__``.  It is used by the
/// ``/`` operator (``__truediv__`` / ``__rtruediv__``) so that
/// ``NotImplemented`` is returned for types like ``CompatiblePathTest.CompatPath``.
fn _try_extract_path_str(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    use pyo3::types::PyString;
    if obj.is_instance_of::<PyString>() || obj.hasattr("__fspath__")? {
        _extract_path_str(obj)
    } else {
        Err(pyo3::exceptions::PyTypeError::new_err(
            "argument should be a str or an os.PathLike object, not '{}'",
        ))
    }
}

/// Extract a string from a Python object that is either a str or a PathLike.
fn _extract_path_str(obj: &Bound<'_, PyAny>) -> PyResult<String> {
    // Reject bytes arguments (CPython pathlib raises TypeError for bytes).
    use pyo3::types::PyBytes;
    if obj.is_instance_of::<PyBytes>() {
        return Err(pyo3::exceptions::PyTypeError::new_err(
            "argument should be a str or an os.PathLike object where __fspath__ returns a str, not 'bytes'",
        ));
    }
    // First try str extraction (only works for str and str subclasses)
    if let Ok(s) = obj.extract::<String>() {
        return Ok(s);
    }
    // PathLike (has __fspath__)
    if let Ok(fspath) = obj.call_method0("__fspath__") {
        let s: String = fspath.extract()?;
        // Also reject PathLike objects returning bytes from __fspath__.
        use pyo3::types::PyBytes;
        if fspath.is_instance_of::<PyBytes>() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "argument should be a str or an os.PathLike object where __fspath__ returns a str, not 'bytes'",
            ));
        }
        return Ok(s);
    }
    // Fallback to str() conversion for compatibility
    Ok(obj.str()?.to_string())
}

/// Returns true if ``authority`` matches the local machine's hostname.
#[cfg(unix)]
fn is_local_hostname(authority: &str) -> bool {
    let mut buf = [0u8; 256];
    let rc = unsafe { libc::gethostname(buf.as_mut_ptr() as *mut _, buf.len()) };
    if rc != 0 {
        return false;
    }
    let hostname = unsafe { std::ffi::CStr::from_ptr(buf.as_ptr() as *const _).to_string_lossy() };
    authority.eq_ignore_ascii_case(&hostname)
}

#[cfg(not(unix))]
fn is_local_hostname(_authority: &str) -> bool {
    // On non-Unix platforms (Windows), we can't easily check the hostname.
    // Fall back to only accepting empty authority and "localhost".
    false
}

/// Parse a ``file:`` URI into a path string.
///
/// Supports:
/// - ``file:///absolute/path`` (POSIX)
/// - ``file:relative/path`` (POSIX)
/// - ``file:///C:/path`` (Windows drive letter)
/// - ``file://host/path`` (non-localhost host → error)
fn parse_file_uri(uri: &str, is_windows: bool) -> PyResult<String> {
    // Strip the "file:" prefix
    let rest = uri
        .strip_prefix("file:")
        .or_else(|| uri.strip_prefix("FILE:"))
        .ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!("URI '{uri}' is not a file: URI"))
        })?;

    // Windows drive letter without authority: file:c:/path or file:c|/path
    if rest.len() >= 2
        && rest.as_bytes()[0].is_ascii_alphabetic()
        && (rest.as_bytes()[1] == b':' || rest.as_bytes()[1] == b'|')
    {
        let drive = rest.as_bytes()[0] as char;
        let rest_path = if rest.len() > 2 { &rest[2..] } else { "" };
        return Ok(format!("{drive}:{rest_path}"));
    }

    // Single-slash drive letter: file:/c|/path → c:/path
    if rest.len() >= 3
        && rest.as_bytes()[0] == b'/'
        && rest.as_bytes()[1].is_ascii_alphabetic()
        && (rest.as_bytes()[2] == b':' || rest.as_bytes()[2] == b'|')
    {
        let drive = rest.as_bytes()[1] as char;
        let rest_path = if rest.len() > 3 { &rest[3..] } else { "" };
        return Ok(format!("{drive}:{rest_path}"));
    }

    // Must have an authority (//) or be an absolute POSIX path
    let authority_rest = match rest.strip_prefix("//") {
        Some(ar) => ar,
        None => {
            if !rest.starts_with('/') {
                return Err(pyo3::exceptions::PyValueError::new_err(format!(
                    "non-local file: URI not supported: '{uri}'"
                )));
            }
            return Ok(rest.to_string());
        }
    };

    // Split authority from path
    let (authority, path_part) = match authority_rest.find('/') {
        Some(idx) => {
            let (auth, path) = authority_rest.split_at(idx);
            (auth, &path[1..])
        }
        None => (authority_rest, ""),
    };

    // Empty authority or localhost → local file
    let is_local = authority.is_empty()
        || authority.eq_ignore_ascii_case("localhost")
        || is_local_hostname(authority);
    if is_local {
        if path_part.is_empty() {
            return Ok("/".to_string());
        }
        // Windows drive letter after local authority: /C:/path or /C|/path
        if path_part.len() >= 3
            && path_part.as_bytes()[0].is_ascii_alphabetic()
            && (path_part.as_bytes()[1] == b':' || path_part.as_bytes()[1] == b'|')
            && path_part.as_bytes()[2] == b'/'
        {
            let drive = path_part.as_bytes()[0] as char;
            let rest_path = &path_part[3..];
            if rest_path.is_empty() {
                return Ok(format!("{drive}:\\"));
            } else {
                return Ok(format!("{drive}:\\{rest_path}"));
            }
        }
        // When authority is empty and path_part has a leading / that isn't
        // a drive letter, treat as UNC (//server/path).
        // file:////server/path → authority="" + path="/server/path" → //server/path
        if authority.is_empty() && path_part.starts_with('/') {
            if path_part.starts_with("//") {
                return Ok(path_part.to_string());
            }
            return Ok(format!("/{path_part}"));
        }
        if path_part.starts_with('/') {
            return Ok(path_part.to_string());
        }
        return Ok(format!("/{path_part}"));
    }

    // Non-local authority → UNC path on Windows, ValueError on POSIX
    if !is_windows {
        return Err(pyo3::exceptions::PyValueError::new_err(format!(
            "non-local file: URI not supported: '{uri}'"
        )));
    }
    if path_part.is_empty() {
        Ok(format!("//{authority}"))
    } else {
        Ok(format!("//{authority}/{path_part}"))
    }
}

/// Create a PyErr for UnsupportedOperation with the given method name message.
fn unsupported_msg(method: &str) -> pyo3::PyErr {
    Python::with_gil(|py| {
        let pathlibrs = py
            .import("pathlibrs")
            .expect("pathlibrs module should be importable");
        let exc = pathlibrs
            .getattr("UnsupportedOperation")
            .expect("UnsupportedOperation should be defined");
        let msg = format!("{method} is unsupported on this system");
        PyErr::from_value(exc.call1((msg,)).unwrap())
    })
}
