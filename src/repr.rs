//! Core data types for path representation and lazy parsing.

use std::ffi::{OsStr, OsString};
use std::sync::OnceLock;

use crate::parsing::parse_path;

/// Platform flavour for path parsing and formatting.
///
/// Determines separator characters, drive letter handling,
/// and other platform-specific path behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PathFlavour {
    /// POSIX-style paths (``/`` separator, no drives).
    Posix,
    /// Windows-style paths (``\\`` or ``/`` separators, drive letters, UNC).
    Windows,
}

/// Parsed components of a path, cached after first access.
///
/// All string fields are substrings of the original path,
/// stored as owned [`OsString`] values for safety.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParsedPath {
    /// Drive component (e.g. ``"C:"`` on Windows, [`None`] on POSIX).
    pub drive: Option<OsString>,
    /// Root component (e.g. ``"/"`` on POSIX, ``"\\"`` on Windows).
    pub root: Option<OsString>,
    /// Non-anchor path components, in order.
    pub parts: Vec<OsString>,
    /// Length of the anchor (drive + root) in the original path bytes.
    pub anchor_length: usize,
    /// Whether this path has a non-empty name component.
    pub has_name: bool,
}

impl ParsedPath {
    /// Return the absolute position of the last part in the original path.
    ///
    /// Used to slice the raw string for zero-copy name/stem/suffix.
    pub fn name_end(&self) -> usize {
        self.anchor_length
            + self
                .parts
                .iter()
                .map(|p| p.len() + 1) // +1 for separator
                .sum::<usize>()
                .saturating_sub(1) // last part has no trailing separator
    }

    /// Compare two parsed paths with Windows case-insensitivity.
    ///
    /// Drives and parts are compared case-insensitively; root is always
    /// a backslash so it is compared exactly.
    pub fn eq_windows(&self, other: &ParsedPath) -> bool {
        // Drives: case-insensitive
        let drives_eq = match (&self.drive, &other.drive) {
            (Some(a), Some(b)) => a
                .as_encoded_bytes()
                .eq_ignore_ascii_case(b.as_encoded_bytes()),
            (None, None) => true,
            _ => false,
        };
        if !drives_eq {
            return false;
        }
        // Root: exact (always a backslash on Windows)
        if self.root != other.root {
            return false;
        }
        // Parts: case-insensitive
        if self.parts.len() != other.parts.len() {
            return false;
        }
        for (a, b) in self.parts.iter().zip(other.parts.iter()) {
            if !a
                .as_encoded_bytes()
                .eq_ignore_ascii_case(b.as_encoded_bytes())
            {
                return false;
            }
        }
        true
    }

    /// Check if two parsed path parts are equal, with optional case-insensitivity.
    pub fn parts_equal(a: &OsString, b: &OsString, windows: bool) -> bool {
        if windows {
            a.as_encoded_bytes()
                .eq_ignore_ascii_case(b.as_encoded_bytes())
        } else {
            a == b
        }
    }
}

/// Lazy-parsed path representation.
///
/// Stores the raw path as an [`OsString`] and parses it on first access
/// via [`OnceLock`]. Parsing is cached so subsequent accesses are free.
///
/// # Thread safety
///
/// `PathRepr` is both [`Send`] and [`Sync`] — all fields are thread-safe
/// and parsing uses interior mutability through [`OnceLock`].
#[derive(Debug, Clone)]
pub struct PathRepr {
    /// The original path string.
    raw: OsString,
    /// Lazily-computed parsed components.
    parsed: OnceLock<Box<ParsedPath>>,
}

impl PathRepr {
    /// Create a new `PathRepr` from a raw path string.
    pub fn new(raw: OsString) -> Self {
        Self {
            raw,
            parsed: OnceLock::new(),
        }
    }

    /// Return the raw path string.
    pub fn raw(&self) -> &OsStr {
        &self.raw
    }

    /// Return the raw path as an [`OsString`] (for ownership transfer).
    pub fn into_raw(self) -> OsString {
        self.raw
    }

    /// Get or compute the parsed representation.
    ///
    /// The first call parses the path; subsequent calls return the cached result.
    pub fn parsed(&self, flavour: PathFlavour) -> &ParsedPath {
        self.parsed
            .get_or_init(|| Box::new(parse_path(&self.raw, flavour)))
    }
}

// Safety: OsString and OnceLock<Box<ParsedPath>> are both Send + Sync.
// ParsedPath only contains OsString/Vec<OsString>/usize/bool, all Send+Sync.
unsafe impl Send for PathRepr {}
unsafe impl Sync for PathRepr {}
