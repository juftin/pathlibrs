//! Python-facing iterator types for path components.
//!
//! Provides ``PartsIter`` (for ``.parts``) and ``ParentsIter`` (for ``.parents``).

use std::ffi::OsString;

use pyo3::prelude::*;

use crate::repr::{PathFlavour, PathRepr};

/// Iterator over the ``.parts`` of a path.
///
/// Yields a tuple: ``(drive, root, part1, part2, ...)`` where ``drive``
/// and ``root`` may be empty strings.
#[pyclass(module = "pathlibrs")]
pub struct PartsIter {
    drive: Option<OsString>,
    root: Option<OsString>,
    parts: Vec<OsString>,
    pos: usize, // 0 = drive, 1 = root, 2+ = parts
}

impl PartsIter {
    /// Create a new parts iterator from a parsed path.
    pub fn new(repr: &PathRepr, flavour: PathFlavour) -> Self {
        let parsed = repr.parsed(flavour);
        Self {
            drive: parsed.drive.clone(),
            root: parsed.root.clone(),
            parts: parsed.parts.clone(),
            pos: 0,
        }
    }
}

#[pymethods]
impl PartsIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        let total = self.parts.len() + 2; // drive + root + parts
        if self.pos >= total {
            return Ok(None);
        }

        let result: PyObject = match self.pos {
            0 => self
                .drive
                .as_ref()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into_pyobject(py)?
                .into(),
            1 => self
                .root
                .as_ref()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default()
                .into_pyobject(py)?
                .into(),
            n => {
                let idx = n - 2;
                self.parts[idx]
                    .to_string_lossy()
                    .into_owned()
                    .into_pyobject(py)?
                    .into()
            }
        };

        self.pos += 1;
        Ok(Some(result))
    }

    fn __len__(&self) -> usize {
        self.parts.len() + 2
    }
}

/// Iterator over the ancestor paths of a path (``.parents``).
///
/// Yields each parent path as a new path object, from the immediate
/// parent up to (and including) the root/anchor.
#[pyclass(module = "pathlibrs")]
pub struct ParentsIter {
    raw: OsString,
    anchor_length: usize,
    flavour: PathFlavour,
    parts: Vec<OsString>,
    /// Current number of parts to include (decreasing).
    part_count: usize,
    /// The Python class to use for constructing path objects.
    cls: PyObject,
}

impl ParentsIter {
    /// Create a new parents iterator from a parsed path.
    pub fn new(repr: &PathRepr, flavour: PathFlavour, cls: PyObject) -> Self {
        let parsed = repr.parsed(flavour);
        Self {
            raw: repr.raw().to_os_string(),
            anchor_length: parsed.anchor_length,
            flavour,
            parts: parsed.parts.clone(),
            part_count: parsed.parts.len(),
            cls,
        }
    }
}

#[pymethods]
impl ParentsIter {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python<'_>) -> PyResult<Option<PyObject>> {
        if self.part_count == 0 {
            return Ok(None);
        }

        // Build the parent path string from raw
        let raw_bytes = self.raw.as_encoded_bytes();

        // Calculate the byte length of the parent path.
        // part_count starts at parts.len() and counts down.
        // For part_count=N, we include N-1 parts (the parent of a path with N parts).
        let mut byte_len = self.anchor_length;
        let n_parts = self.part_count.saturating_sub(1);
        for (i, part) in self.parts.iter().enumerate().take(n_parts) {
            if i > 0 || self.anchor_length > 0 {
                // Count separator before the part (anchor already ends with sep)
                byte_len += 1;
            }
            byte_len += part.len();
        }

        // Trim trailing separators, but never go below the anchor length
        let mut end = byte_len.min(raw_bytes.len());
        let is_win = self.flavour == PathFlavour::Windows;
        while end > self.anchor_length {
            let b = raw_bytes[end - 1];
            if b == b'/' || is_win && b == b'\\' {
                end -= 1;
            } else {
                break;
            }
        }

        let parent_bytes = &raw_bytes[..end];
        let parent_str = crate::from_os_bytes(parent_bytes).to_string_lossy();

        // Construct a new path object using the stored class
        let args = pyo3::types::PyTuple::new(py, &[pyo3::types::PyString::new(py, &parent_str)])?;
        let result = self.cls.call1(py, args)?;

        self.part_count = self.part_count.saturating_sub(1);
        Ok(Some(result))
    }
}
