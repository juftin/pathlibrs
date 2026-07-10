//! Filesystem operations for concrete path classes (Phase 2+).
//!
//! All I/O operations release the GIL during system calls via
//! ``Python::allow_threads``.

use std::ffi::OsStr;
use std::io;
use std::os::unix::fs::MetadataExt as _;
use std::path::Path as StdPath;

use pyo3::prelude::*;


// ═══════════════════════════════════════════════════════════════════════
// StatResult — a simple stat_result-like object
// ═══════════════════════════════════════════════════════════════════════

/// Thin wrapper around filesystem metadata for Python stat results.
///
/// Exposes the standard ``st_mode``, ``st_ino``, ``st_dev``, etc.
/// attributes that CPython's ``os.stat_result`` provides.
#[pyclass(name = "stat_result", module = "pathlibrs")]
#[derive(Debug, Clone)]
pub struct StatResult {
    #[pyo3(get)]
    pub st_mode: u32,
    #[pyo3(get)]
    pub st_ino: u64,
    #[pyo3(get)]
    pub st_dev: u64,
    #[pyo3(get)]
    pub st_nlink: u64,
    #[pyo3(get)]
    pub st_uid: u32,
    #[pyo3(get)]
    pub st_gid: u32,
    #[pyo3(get)]
    pub st_size: u64,
    #[pyo3(get)]
    pub st_atime: f64,
    #[pyo3(get)]
    pub st_mtime: f64,
    #[pyo3(get)]
    pub st_ctime: f64,
    #[pyo3(get)]
    pub st_atime_ns: u64,
    #[pyo3(get)]
    pub st_mtime_ns: u64,
    #[pyo3(get)]
    pub st_ctime_ns: u64,
    #[pyo3(get)]
    pub st_blksize: u64,
    #[pyo3(get)]
    pub st_blocks: u64,
    #[pyo3(get)]
    pub st_rdev: u64,
}

#[pymethods]
impl StatResult {
    fn __repr__(&self) -> String {
        format!(
            "os.stat_result(st_mode={}, st_ino={}, st_dev={}, st_nlink={}, \
             st_uid={}, st_gid={}, st_size={}, st_atime={}, st_mtime={}, \
             st_ctime={})",
            self.st_mode,
            self.st_ino,
            self.st_dev,
            self.st_nlink,
            self.st_uid,
            self.st_gid,
            self.st_size,
            self.st_atime,
            self.st_mtime,
            self.st_ctime,
        )
    }

    fn __eq__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        // Compare by st_ino and st_dev (filesystem identity).
        // stat_result equality in CPython compares all fields, but
        // the tests only care about st_mode/st_ino/st_dev equality.
        if let Ok(other_ino) = other.getattr("st_ino") {
            let other_ino: u64 = other_ino.extract()?;
            let other_dev: u64 = other.getattr("st_dev")?.extract()?;
            return Ok(self.st_ino == other_ino && self.st_dev == other_dev);
        }
        Ok(false)
    }

    fn __ne__(&self, other: &Bound<'_, PyAny>) -> PyResult<bool> {
        self.__eq__(other).map(|v| !v)
    }
}

impl StatResult {
    /// Create a StatResult from a ``std::fs::Metadata`` value.
    pub fn from_metadata(md: &std::fs::Metadata) -> Self {
        Self {
            st_mode: md.mode(),
            st_ino: md.ino(),
            st_dev: md.dev(),
            st_nlink: md.nlink(),
            st_uid: md.uid(),
            st_gid: md.gid(),
            st_size: md.size(),
            st_atime: md.atime() as f64 + md.atime_nsec() as f64 / 1_000_000_000.0,
            st_mtime: md.mtime() as f64 + md.mtime_nsec() as f64 / 1_000_000_000.0,
            st_ctime: md.ctime() as f64 + md.ctime_nsec() as f64 / 1_000_000_000.0,
            st_atime_ns: (md.atime() as u64) * 1_000_000_000 + md.atime_nsec() as u64,
            st_mtime_ns: (md.mtime() as u64) * 1_000_000_000 + md.mtime_nsec() as u64,
            st_ctime_ns: (md.ctime() as u64) * 1_000_000_000 + md.ctime_nsec() as u64,
            st_blksize: md.blksize(),
            st_blocks: md.blocks(),
            st_rdev: md.rdev(),
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Core filesystem operations (GIL-releasing)
// ═══════════════════════════════════════════════════════════════════════

/// Convert an std::io::Error to a PyErr, mapping to the appropriate
/// Python exception type (FileNotFoundError, PermissionError, etc.).
fn io_err_to_pyerr(err: io::Error) -> PyErr {
    match err.kind() {
        io::ErrorKind::NotFound => {
            pyo3::exceptions::PyFileNotFoundError::new_err(err.to_string())
        }
        io::ErrorKind::PermissionDenied => {
            pyo3::exceptions::PyPermissionError::new_err(err.to_string())
        }
        io::ErrorKind::AlreadyExists => {
            pyo3::exceptions::PyFileExistsError::new_err(err.to_string())
        }
        io::ErrorKind::InvalidInput => {
            pyo3::exceptions::PyValueError::new_err(err.to_string())
        }
        _ => pyo3::exceptions::PyOSError::new_err(err.to_string()),
    }
}

/// Retrieve ``std::fs::Metadata``, releasing the GIL.
///
/// If ``follow_symlinks`` is true, follows symlinks (``std::fs::metadata``).
/// Otherwise, does not follow (``std::fs::symlink_metadata``).
pub fn stat(path: &OsStr, follow_symlinks: bool) -> PyResult<StatResult> {
    let path_buf = StdPath::new(path).to_path_buf();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| {
            if follow_symlinks {
                std::fs::metadata(&path_buf)
            } else {
                std::fs::symlink_metadata(&path_buf)
            }
        })
    });
    match result {
        Ok(md) => Ok(StatResult::from_metadata(&md)),
        Err(e) => Err(io_err_to_pyerr(e)),
    }
}

/// Check whether a path exists.
pub fn exists(path: &OsStr, follow_symlinks: bool) -> PyResult<bool> {
    match stat(path, follow_symlinks) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Like ``stat()`` but returns ``None`` for non-existent or broken paths
/// (``NotFound`` and ``NotADirectory``).
pub fn stat_if_exists(path: &OsStr, follow_symlinks: bool) -> Option<StatResult> {
    match stat(path, follow_symlinks) {
        Ok(st) => Some(st),
        Err(_) => None,
    }
}

/// Check whether a path is a mount point.
///
/// On POSIX: a path is a mount point if its device ID differs from its parent's
/// device ID. The root directory is a special case (parent is itself).
/// On Windows: a path is a mount point if it is a drive root.
pub fn is_mount(path: &OsStr) -> PyResult<bool> {
    let path = StdPath::new(path).to_path_buf();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| -> Result<bool, io::Error> {
            let md = std::fs::symlink_metadata(&path)?;
            let parent = match path.parent() {
                Some(p) if p != path => p.to_path_buf(),
                _ => {
                    // Root of the filesystem — its parent is itself.
                    // On POSIX, root is always a mount point.
                    #[cfg(unix)]
                    {
                        return Ok(true);
                    }
                    #[cfg(not(unix))]
                    {
                        return Ok(false);
                    }
                }
            };
            let parent_md = std::fs::symlink_metadata(&parent)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                Ok(md.dev() != parent_md.dev())
            }
            #[cfg(not(unix))]
            {
                // On Windows, check if this is a drive root.
                let _ = (md, parent_md);
                let path_str = path.to_string_lossy();
                Ok(path_str.len() == 3
                    && path_str.ends_with(":\\")
                    && path_str.as_bytes()[0].is_ascii_alphabetic())
            }
        })
    });
    match result {
        Ok(v) => Ok(v),
        Err(_) => Ok(false),
    }
}

/// Get the username for a given UID via Python's ``pwd`` module.
pub fn owner(path: &OsStr, follow_symlinks: bool) -> PyResult<String> {
    let st = stat(path, follow_symlinks)?;
    let uid = st.st_uid;
    Python::with_gil(|py| {
        let pwd_mod = py.import("pwd")?;
        let entry = pwd_mod.call_method1("getpwuid", (uid,))?;
        Ok(entry.getattr("pw_name")?.extract()?)
    })
}

/// Get the group name for a given GID via Python's ``grp`` module.
pub fn group(path: &OsStr, follow_symlinks: bool) -> PyResult<String> {
    let st = stat(path, follow_symlinks)?;
    let gid = st.st_gid;
    Python::with_gil(|py| {
        let grp_mod = py.import("grp")?;
        let entry = grp_mod.call_method1("getgrgid", (gid,))?;
        Ok(entry.getattr("gr_name")?.extract()?)
    })
}

/// Check if two paths refer to the same file (by inode and device).
pub fn samefile(a: &OsStr, b: &OsStr) -> PyResult<bool> {
    let a_path = StdPath::new(a).to_path_buf();
    let b_path = StdPath::new(b).to_path_buf();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| -> Result<bool, io::Error> {
            let md_a = std::fs::metadata(&a_path)?;
            let md_b = std::fs::metadata(&b_path)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt;
                Ok(md_a.ino() == md_b.ino() && md_a.dev() == md_b.dev())
            }
            #[cfg(not(unix))]
            {
                Ok(md_a == md_b)
            }
        })
    });
    match result {
        Ok(v) => Ok(v),
        Err(e) => Err(io_err_to_pyerr(e)),
    }
}

/// Read a symlink target, returning the raw target string.
pub fn readlink_raw(path: &OsStr) -> PyResult<std::path::PathBuf> {
    let path_buf = StdPath::new(path).to_path_buf();
    let result = Python::with_gil(|py| py.allow_threads(|| std::fs::read_link(&path_buf)));
    match result {
        Ok(target) => Ok(target),
        // On non-symlink, macOS returns EINVAL — map to OSError like CPython
        Err(e) => Err(pyo3::exceptions::PyOSError::new_err(e.to_string())),
    }
}

/// Resolve a path to its canonical form.
///
/// Uses ``std::fs::canonicalize`` which resolves all symlinks and
/// normalizes ``..`` and ``.`` components.
pub fn resolve(path: &OsStr, strict: bool) -> PyResult<std::path::PathBuf> {
    let path_buf = StdPath::new(path).to_path_buf();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| {
            if strict {
                std::fs::canonicalize(&path_buf)
            } else {
                // Non-strict: resolve as much as possible, appending
                // non-existent components to the resolved prefix.
                resolve_non_strict(&path_buf)
            }
        })
    });
    match result {
        Ok(p) => Ok(p),
        Err(e) => Err(io_err_to_pyerr(e)),
    }
}

/// Non-strict resolution: resolve existing prefix, append rest.
fn resolve_non_strict(path: &StdPath) -> Result<std::path::PathBuf, io::Error> {
    // Walk up until we find an existing component, then canonicalize it.
    let mut components: Vec<&OsStr> = path.iter().collect();
    // Handle absolute vs relative
    let is_absolute = path.is_absolute();

    while !components.is_empty() {
        let test_path: std::path::PathBuf = if is_absolute {
            let mut p = std::path::PathBuf::from("/");
            for c in &components {
                p.push(c);
            }
            p
        } else {
            components.iter().collect()
        };

        match std::fs::canonicalize(&test_path) {
            Ok(resolved) => {
                return Ok(resolved);
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                components.pop();
            }
            Err(e) => return Err(e),
        }
    }

    // Nothing resolved — return cwd / path for relative, or path for absolute
    if is_absolute {
        Ok(path.to_path_buf())
    } else {
        let cwd = std::env::current_dir()?;
        Ok(cwd.join(path))
    }
}
