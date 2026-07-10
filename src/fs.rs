//! Filesystem operations for concrete path classes (Phase 2+).
//!
//! All I/O operations release the GIL during system calls via
//! ``Python::allow_threads``.

use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path as StdPath;
use std::sync::OnceLock;

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
    #[cfg(unix)]
    pub fn from_metadata(md: &std::fs::Metadata) -> Self {
        use std::os::unix::fs::MetadataExt as _;
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

    /// Create a StatResult from a ``std::fs::Metadata`` value (Windows).
    #[cfg(not(unix))]
    pub fn from_metadata(md: &std::fs::Metadata) -> Self {
        use std::os::windows::fs::MetadataExt as _;
        // Windows MetadataExt (stable) provides: file_attributes(),
        // creation_time(), last_access_time(), last_write_time(), file_size()
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        let atime = secs_since_epoch(md.last_access_time());
        let mtime = secs_since_epoch(md.last_write_time());
        let ctime = secs_since_epoch(md.creation_time());
        let attrs = md.file_attributes();
        let file_type = if attrs & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            0o120000 // S_IFLNK
        } else if md.is_dir() {
            0o040000 // S_IFDIR
        } else {
            0o100000 // S_IFREG
        };
        Self {
            st_mode: 0o666 | file_type,
            st_ino: 0,
            st_dev: 0,
            st_nlink: 1,
            st_uid: 0,
            st_gid: 0,
            st_size: md.file_size(),
            st_atime: atime,
            st_mtime: mtime,
            st_ctime: ctime,
            st_atime_ns: (atime * 1_000_000_000.0) as u64,
            st_mtime_ns: (mtime * 1_000_000_000.0) as u64,
            st_ctime_ns: (ctime * 1_000_000_000.0) as u64,
            st_blksize: 0,
            st_blocks: 0,
            st_rdev: 0,
        }
    }
}

/// Convert Windows FILETIME to seconds since Unix epoch.
#[cfg(not(unix))]
fn secs_since_epoch(ft: u64) -> f64 {
    // FILETIME is 100-nanosecond intervals since 1601-01-01
    // Unix epoch is 1970-01-01. Difference is 11644473600 seconds.
    const WINDOWS_TO_UNIX_EPOCH: u64 = 11_644_473_600;
    (ft / 10_000_000) as f64 - WINDOWS_TO_UNIX_EPOCH as f64
}

// ═══════════════════════════════════════════════════════════════════════
// Core filesystem operations (GIL-releasing)
// ═══════════════════════════════════════════════════════════════════════

/// Convert an std::io::Error to a PyErr, mapping to the appropriate
/// Python exception type (FileNotFoundError, PermissionError, etc.).
fn io_err_to_pyerr(err: io::Error) -> PyErr {
    match err.kind() {
        io::ErrorKind::NotFound => pyo3::exceptions::PyFileNotFoundError::new_err(err.to_string()),
        io::ErrorKind::PermissionDenied => {
            pyo3::exceptions::PyPermissionError::new_err(err.to_string())
        }
        io::ErrorKind::AlreadyExists => {
            pyo3::exceptions::PyFileExistsError::new_err(err.to_string())
        }
        io::ErrorKind::InvalidInput => pyo3::exceptions::PyValueError::new_err(err.to_string()),
        _ => pyo3::exceptions::PyOSError::new_err(err.to_string()),
    }
}

/// Retrieve ``std::fs::Metadata``, releasing the GIL.
///
/// If ``follow_symlinks`` is true, follows symlinks (``std::fs::metadata``).
/// Otherwise, does not follow (``std::fs::symlink_metadata``).
///
/// On Windows, delegates to Python's ``os.stat()`` / ``os.lstat()`` for
/// field-for-field accuracy with CPython (``st_ino``, ``st_dev``, ``st_mode``,
/// etc. are not available from ``std::fs::Metadata`` on Windows).
pub fn stat(path: &OsStr, follow_symlinks: bool) -> PyResult<StatResult> {
    #[cfg(unix)]
    {
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
    #[cfg(windows)]
    {
        stat_windows(path, follow_symlinks)
    }
}

/// Retrieve file status on Windows via Python's ``os.stat()`` / ``os.lstat()``.
///
/// ``std::fs::Metadata`` on Windows does not provide ``st_ino``, ``st_dev``,
/// or symlink-aware ``st_mode``.  Delegating to CPython's own stat
/// implementation ensures field-for-field compatibility with ``os.stat_result``.
#[cfg(windows)]
fn stat_windows(path: &OsStr, follow_symlinks: bool) -> PyResult<StatResult> {
    Python::with_gil(|py| {
        let path_str = path.to_string_lossy();
        let os = py.import("os")?;
        let func_name = if follow_symlinks { "stat" } else { "lstat" };
        let result = os.call_method1(func_name, (&*path_str,))?;

        // Extract fields from Python's os.stat_result
        Ok(StatResult {
            st_mode: result.getattr("st_mode")?.extract()?,
            st_ino: result.getattr("st_ino")?.extract()?,
            st_dev: result.getattr("st_dev")?.extract()?,
            st_nlink: result.getattr("st_nlink")?.extract()?,
            st_uid: result.getattr("st_uid")?.extract()?,
            st_gid: result.getattr("st_gid")?.extract()?,
            st_size: result.getattr("st_size")?.extract()?,
            st_atime: result.getattr("st_atime")?.extract()?,
            st_mtime: result.getattr("st_mtime")?.extract()?,
            st_ctime: result.getattr("st_ctime")?.extract()?,
            st_atime_ns: result.getattr("st_atime_ns")?.extract::<i64>()? as u64,
            st_mtime_ns: result.getattr("st_mtime_ns")?.extract::<i64>()? as u64,
            st_ctime_ns: result.getattr("st_ctime_ns")?.extract::<i64>()? as u64,
            st_blksize: result
                .getattr("st_blksize")
                .map(|v| v.extract::<i64>().unwrap_or(0))
                .unwrap_or(0) as u64,
            st_blocks: result
                .getattr("st_blocks")
                .map(|v| v.extract::<i64>().unwrap_or(0))
                .unwrap_or(0) as u64,
            st_rdev: result
                .getattr("st_rdev")
                .map(|v| v.extract::<i64>().unwrap_or(0))
                .unwrap_or(0) as u64,
        })
    })
}

/// Check whether a path exists.
///
/// On Unix, delegates to ``stat()``; on Windows, delegates to Python's
/// ``os.path.exists()`` / ``os.path.lexists()`` for exact CPython behavior.
#[cfg(unix)]
pub fn exists(path: &OsStr, follow_symlinks: bool) -> PyResult<bool> {
    match stat(path, follow_symlinks) {
        Ok(_) => Ok(true),
        Err(_) => Ok(false),
    }
}

/// Check whether a path exists (Windows: delegates to Python ``os.path``).
#[cfg(windows)]
pub fn exists(path: &OsStr, follow_symlinks: bool) -> PyResult<bool> {
    Python::with_gil(|py| {
        let os_path = py.import("os.path")?;
        let path_str = path.to_string_lossy();
        if follow_symlinks {
            os_path
                .call_method1("exists", (path_str.as_ref(),))?
                .extract()
        } else {
            os_path
                .call_method1("lexists", (path_str.as_ref(),))?
                .extract()
        }
    })
}

/// Like ``stat()`` but returns ``None`` for non-existent or broken paths
/// (``NotFound`` and ``NotADirectory``).
pub fn stat_if_exists(path: &OsStr, follow_symlinks: bool) -> Option<StatResult> {
    stat(path, follow_symlinks).ok()
}

/// Check whether a path is a mount point.
///
/// On POSIX: a path is a mount point if its device ID differs from its parent's.
/// On Windows: a path is a mount point if it is a drive root.
pub fn is_mount(path: &OsStr) -> PyResult<bool> {
    let path = StdPath::new(path).to_path_buf();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| -> Result<bool, io::Error> {
            let md = std::fs::symlink_metadata(&path)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::MetadataExt as _;
                let parent = match path.parent() {
                    Some(p) if p != path => p.to_path_buf(),
                    _ => return Ok(true), // Root is always a mount point
                };
                let parent_md = std::fs::symlink_metadata(&parent)?;
                Ok(md.dev() != parent_md.dev())
            }
            #[cfg(windows)]
            {
                let _ = md;
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
        entry.getattr("pw_name")?.extract()
    })
}

/// Get the group name for a given GID via Python's ``grp`` module.
pub fn group(path: &OsStr, follow_symlinks: bool) -> PyResult<String> {
    let st = stat(path, follow_symlinks)?;
    let gid = st.st_gid;
    Python::with_gil(|py| {
        let grp_mod = py.import("grp")?;
        let entry = grp_mod.call_method1("getgrgid", (gid,))?;
        entry.getattr("gr_name")?.extract()
    })
}

/// Check if two paths refer to the same file.
#[cfg(unix)]
pub fn samefile(a: &OsStr, b: &OsStr) -> PyResult<bool> {
    use std::os::unix::fs::MetadataExt as _;
    let a_path = StdPath::new(a).to_path_buf();
    let b_path = StdPath::new(b).to_path_buf();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| -> Result<bool, io::Error> {
            let md_a = std::fs::metadata(&a_path)?;
            let md_b = std::fs::metadata(&b_path)?;
            Ok(md_a.ino() == md_b.ino() && md_a.dev() == md_b.dev())
        })
    });
    match result {
        Ok(v) => Ok(v),
        Err(e) => Err(io_err_to_pyerr(e)),
    }
}

/// Check if two paths refer to the same file (Windows stub).
#[cfg(not(unix))]
pub fn samefile(a: &OsStr, b: &OsStr) -> PyResult<bool> {
    let a_path = StdPath::new(a).to_path_buf();
    let b_path = StdPath::new(b).to_path_buf();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| -> Result<bool, io::Error> {
            let md_a = std::fs::metadata(&a_path)?;
            let md_b = std::fs::metadata(&b_path)?;
            // Compare canonical paths on Windows
            let canon_a = std::fs::canonicalize(&a_path)?;
            let canon_b = std::fs::canonicalize(&b_path)?;
            let _ = (md_a, md_b);
            Ok(canon_a == canon_b)
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
        Err(e) => Err(pyo3::exceptions::PyOSError::new_err(e.to_string())),
    }
}

/// Resolve a path to its canonical form.
pub fn resolve(path: &OsStr, strict: bool) -> PyResult<std::path::PathBuf> {
    let path_buf = StdPath::new(path).to_path_buf();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| {
            if strict {
                std::fs::canonicalize(&path_buf)
            } else {
                resolve_non_strict(&path_buf)
            }
        })
    });
    match result {
        Ok(p) => Ok(p),
        Err(e) => Err(io_err_to_pyerr(e)),
    }
}

// ═══════════════════════════════════════════════════════════════════════
// PathInfo — cached stat result (CPython 3.12+)
// ═══════════════════════════════════════════════════════════════════════

/// Cached stat result for a path, matching CPython 3.12+ ``PathInfo``.
///
/// Once computed, the stat result is immutable. All methods return ``False``
/// on ``OSError`` rather than raising.
#[pyclass(name = "PathInfo", module = "pathlibrs")]
#[derive(Debug)]
pub struct PathInfo {
    raw_path: OsString,
    stat_cache: OnceLock<Option<StatResult>>,
    lstat_cache: OnceLock<Option<StatResult>>,
}

impl PathInfo {
    /// Return cached stat or compute and cache it.
    fn get_stat(&self, follow_symlinks: bool) -> Option<&StatResult> {
        let cache = if follow_symlinks {
            &self.stat_cache
        } else {
            &self.lstat_cache
        };
        cache
            .get_or_init(|| stat(&self.raw_path, follow_symlinks).ok())
            .as_ref()
    }
}

#[pymethods]
impl PathInfo {
    /// Create a new PathInfo for the given raw path.
    #[new]
    pub fn new(raw_path: &str) -> Self {
        PathInfo {
            raw_path: OsString::from(raw_path),
            stat_cache: OnceLock::new(),
            lstat_cache: OnceLock::new(),
        }
    }

    /// Check whether the path exists (uses cached stat).
    #[pyo3(signature = (*, follow_symlinks = true))]
    fn exists(&self, follow_symlinks: bool) -> bool {
        self.get_stat(follow_symlinks).is_some()
    }

    /// Check whether the path is a directory (uses cached stat).
    #[pyo3(signature = (*, follow_symlinks = true))]
    fn is_dir(&self, follow_symlinks: bool) -> bool {
        match self.get_stat(follow_symlinks) {
            Some(st) => (st.st_mode & 0o170000) == 0o040000,
            None => false,
        }
    }

    /// Check whether the path is a regular file (uses cached stat).
    #[pyo3(signature = (*, follow_symlinks = true))]
    fn is_file(&self, follow_symlinks: bool) -> bool {
        match self.get_stat(follow_symlinks) {
            Some(st) => (st.st_mode & 0o170000) == 0o100000,
            None => false,
        }
    }

    /// Check whether the path is a symbolic link (uses cached lstat).
    fn is_symlink(&self) -> bool {
        match self.get_stat(false) {
            Some(st) => (st.st_mode & 0o170000) == 0o120000,
            None => false,
        }
    }

    fn __repr__(&self) -> String {
        format!("PathInfo('{}')", self.raw_path.to_string_lossy())
    }
}

/// Non-strict resolution: resolve existing prefix, append rest.
fn resolve_non_strict(path: &StdPath) -> Result<std::path::PathBuf, io::Error> {
    let mut components: Vec<&OsStr> = path.iter().collect();
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
            Ok(resolved) => return Ok(resolved),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                components.pop();
            }
            Err(e) => return Err(e),
        }
    }

    if is_absolute {
        Ok(path.to_path_buf())
    } else {
        let cwd = std::env::current_dir()?;
        Ok(cwd.join(path))
    }
}
