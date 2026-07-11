//! Filesystem operations for concrete path classes (Phase 2+).
//!
//! All I/O operations release the GIL during system calls via
//! ``Python::allow_threads``.

use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::Path as StdPath;
use std::sync::OnceLock;

use pyo3::prelude::*;
use pyo3::types::PyBytes;

// Thread-local set for copy symlink cycle detection.
thread_local! {
    static COPY_VISITED: std::cell::RefCell<std::collections::HashSet<std::path::PathBuf>> =
        std::cell::RefCell::new(std::collections::HashSet::new());
}

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
///
/// Sets ``errno`` on the exception so CPython tests that check
/// ``exception.errno == errno.ENOENT`` pass.
fn io_err_to_pyerr(err: io::Error) -> PyErr {
    let raw_os_error = err.raw_os_error();
    let msg = err.to_string();
    Python::with_gil(|py| {
        let exc_type: Bound<'_, pyo3::types::PyType> = match err.kind() {
            io::ErrorKind::NotFound => py.get_type::<pyo3::exceptions::PyFileNotFoundError>(),
            io::ErrorKind::PermissionDenied => py.get_type::<pyo3::exceptions::PyPermissionError>(),
            io::ErrorKind::AlreadyExists => py.get_type::<pyo3::exceptions::PyFileExistsError>(),
            _ => py.get_type::<pyo3::exceptions::PyOSError>(),
        };
        let errno_val: PyObject = raw_os_error.into_pyobject(py).unwrap().into_any().unbind();
        PyErr::from_type(exc_type, (errno_val, msg))
    })
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

// ═══════════════════════════════════════════════════════════════════════
// Phase 3: Directory Mutations
// ═══════════════════════════════════════════════════════════════════════

/// Create a directory at ``path``.
///
/// Parameters
/// ----------
/// mode : u32
///     Permission mode (Unix-only; ignored on Windows).
/// parents : bool
///     If ``True``, create all missing parent directories.
/// exist_ok : bool
///     If ``True``, do not raise when the directory already exists.
pub fn mkdir(path: &OsStr, mode: u32, parents: bool, exist_ok: bool) -> PyResult<()> {
    let path_buf = StdPath::new(path).to_path_buf();

    // Check if the path already exists
    if path_buf.exists() {
        if path_buf.is_dir() {
            if !exist_ok {
                return Err(file_exists_error(format!(
                    "'{}' already exists",
                    path_buf.display()
                )));
            }
            // Directory exists and exist_ok is true — nothing to do
            return Ok(());
        }
        // Path exists but is not a directory (e.g., a file)
        return Err(file_exists_error(format!(
            "'{}' exists and is not a directory",
            path_buf.display()
        )));
    }

    if parents {
        // Check each parent component — if any exists as a file, raise
        let mut ancestor = path_buf.clone();
        while let Some(parent) = ancestor.parent() {
            if parent.as_os_str().is_empty() {
                break;
            }
            if parent.exists() && !parent.is_dir() {
                return Err(not_a_directory_error(format!(
                    "'{}' exists and is not a directory",
                    parent.display()
                )));
            }
            ancestor = parent.to_path_buf();
        }
    }

    let result = Python::with_gil(|py| {
        py.allow_threads(|| -> Result<(), io::Error> {
            if parents {
                std::fs::create_dir_all(&path_buf)?;
            } else {
                std::fs::create_dir(&path_buf)?;
            }
            Ok(())
        })
    });

    match result {
        Ok(()) => {
            // Set permissions after creation (Unix-only).
            // Only override when the caller explicitly requested a mode.
            #[cfg(unix)]
            {
                if mode != 0o777 {
                    use std::os::unix::fs::PermissionsExt;
                    let perms = std::fs::Permissions::from_mode(mode);
                    let perm_result = Python::with_gil(|py| {
                        py.allow_threads(|| std::fs::set_permissions(&path_buf, perms))
                    });
                    if let Err(e) = perm_result {
                        return Err(io_err_to_pyerr(e));
                    }
                }
            }
            #[cfg(not(unix))]
            {
                let _ = mode;
            }
            Ok(())
        }
        Err(e) => Err(io_err_to_pyerr(e)),
    }
}

/// Create a FileExistsError with errno set to EEXIST.
fn file_exists_error(msg: String) -> PyErr {
    Python::with_gil(|py| {
        let exc_type = py.get_type::<pyo3::exceptions::PyFileExistsError>();
        let errno_val = 17i32.into_pyobject(py).unwrap().into_any().unbind();
        PyErr::from_type(exc_type, (errno_val, msg))
    })
}

/// Create a NotADirectoryError with errno set to ENOTDIR.
fn not_a_directory_error(msg: String) -> PyErr {
    Python::with_gil(|py| {
        let exc_type = py.get_type::<pyo3::exceptions::PyOSError>();
        let errno_val = 20i32.into_pyobject(py).unwrap().into_any().unbind();
        PyErr::from_type(exc_type, (errno_val, msg))
    })
}

/// Remove an empty directory at ``path``.
pub fn rmdir(path: &OsStr) -> PyResult<()> {
    let path_buf = StdPath::new(path).to_path_buf();
    let result = Python::with_gil(|py| py.allow_threads(|| std::fs::remove_dir(&path_buf)));
    match result {
        Ok(()) => Ok(()),
        Err(e) => Err(io_err_to_pyerr(e)),
    }
}

/// Change file mode (permissions).
///
/// On Unix, sets the full permission bits. On Windows, only the read-only
/// flag is supported.
pub fn chmod(path: &OsStr, mode: u32, follow_symlinks: bool) -> PyResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path_buf = StdPath::new(path).to_path_buf();
        if follow_symlinks {
            let perms = std::fs::Permissions::from_mode(mode);
            let result = Python::with_gil(|py| {
                py.allow_threads(|| std::fs::set_permissions(&path_buf, perms))
            });
            result.map_err(io_err_to_pyerr)
        } else {
            // lchmod: change permissions on the symlink itself.
            // Use libc::fchmodat with AT_SYMLINK_NOFOLLOW.
            lchmod_raw(path, mode)
        }
    }
    #[cfg(not(unix))]
    {
        let _ = follow_symlinks;
        // On Windows, delegate to Python's os.chmod
        Python::with_gil(|py| {
            let path_str = path.to_string_lossy();
            let os_mod = py.import("os")?;
            os_mod.call_method1("chmod", (path_str.as_ref(), mode))?;
            Ok(())
        })
    }
}

/// Change permissions on a symlink without following it (Unix only).
#[cfg(unix)]
fn lchmod_raw(path: &OsStr, mode: u32) -> PyResult<()> {
    use std::ffi::CString;

    let path_bytes = path.as_encoded_bytes();
    let c_path = CString::new(path_bytes)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("invalid path: {e}")))?;

    // macOS/BSD: fchmodat with AT_SYMLINK_NOFOLLOW
    // Linux: lchmod is not supported (raises NotImplementedError)
    let result = Python::with_gil(|py| {
        py.allow_threads(|| unsafe {
            let ret = libc::fchmodat(
                libc::AT_FDCWD,
                c_path.as_ptr(),
                mode as libc::mode_t,
                libc::AT_SYMLINK_NOFOLLOW,
            );
            if ret != 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
    });

    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            // EOPNOTSUPP/ENOTSUP: lchmod not supported (Linux)
            if e.raw_os_error() == Some(libc::EOPNOTSUPP) || e.raw_os_error() == Some(libc::ENOTSUP)
            {
                // Fall back to Python's os.lchmod for the error message
                Python::with_gil(|py| {
                    let path_str = path.to_string_lossy();
                    let os_mod = py.import("os")?;
                    os_mod.call_method1("lchmod", (path_str.as_ref(), mode))?;
                    Ok(())
                })
            } else {
                Err(io_err_to_pyerr(e))
            }
        }
    }
}

/// Change permissions on a symlink without following it (non-Unix stub).
#[cfg(not(unix))]
#[allow(dead_code)]
fn lchmod_raw(_path: &OsStr, _mode: u32) -> PyResult<()> {
    Err(pyo3::exceptions::PyNotImplementedError::new_err(
        "lchmod is not available on this platform",
    ))
}

// ═══════════════════════════════════════════════════════════════════════
// Phase 3: File Mutations
// ═══════════════════════════════════════════════════════════════════════

/// Create a file or update its modification time.
pub fn touch(path: &OsStr, mode: u32, exist_ok: bool) -> PyResult<()> {
    let path_buf = StdPath::new(path).to_path_buf();
    let exists = path_buf.exists();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| -> Result<(), io::Error> {
            if exists {
                if !exist_ok {
                    return Err(io::Error::new(
                        io::ErrorKind::AlreadyExists,
                        format!("'{}' already exists", path_buf.display()),
                    ));
                }
                // Update modification time only
                let file = std::fs::OpenOptions::new().write(true).open(&path_buf)?;
                file.set_modified(std::time::SystemTime::now())?;
            } else {
                // Create new empty file
                std::fs::File::create(&path_buf)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if mode != 0o666 {
                        let perms = std::fs::Permissions::from_mode(mode);
                        std::fs::set_permissions(&path_buf, perms)?;
                    }
                }
                let _ = mode;
            }
            Ok(())
        })
    });

    result.map_err(io_err_to_pyerr)
}

/// Remove (unlink) a file or symlink.
pub fn unlink(path: &OsStr, missing_ok: bool) -> PyResult<()> {
    let path_buf = StdPath::new(path).to_path_buf();
    let result = Python::with_gil(|py| py.allow_threads(|| std::fs::remove_file(&path_buf)));
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            if missing_ok && e.kind() == io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(io_err_to_pyerr(e))
            }
        }
    }
}

/// Rename a file or directory (same filesystem).
///
/// ``rename()`` replaces the destination on POSIX but raises on Windows
/// if the destination exists.
pub fn rename(src: &OsStr, dst: &OsStr) -> PyResult<()> {
    let src_buf = StdPath::new(src).to_path_buf();
    let dst_buf = StdPath::new(dst).to_path_buf();
    let result = Python::with_gil(|py| py.allow_threads(|| std::fs::rename(&src_buf, &dst_buf)));
    match result {
        Ok(()) => Ok(()),
        Err(e) => Err(io_err_to_pyerr(e)),
    }
}

/// Replace one file or directory with another (cross-platform atomic).
///
/// On POSIX, ``rename()`` is atomic and replaces the destination.
/// On Windows, ``std::fs::rename`` fails if ``dst`` exists, so we
/// must remove it first.
pub fn replace(src: &OsStr, dst: &OsStr) -> PyResult<()> {
    let src_buf = StdPath::new(src).to_path_buf();
    let dst_buf = StdPath::new(dst).to_path_buf();

    #[cfg(windows)]
    {
        // On Windows, std::fs::rename fails if dst exists.
        // Remove dst first if it exists.
        let result = Python::with_gil(|py| {
            py.allow_threads(|| -> Result<(), io::Error> {
                // Try to remove dst first (might be file or empty dir)
                if dst_buf.is_dir() {
                    std::fs::remove_dir(&dst_buf)?;
                } else if dst_buf.exists() {
                    std::fs::remove_file(&dst_buf)?;
                }
                std::fs::rename(&src_buf, &dst_buf)?;
                Ok(())
            })
        });
        result.map_err(io_err_to_pyerr)
    }

    #[cfg(not(windows))]
    {
        let result =
            Python::with_gil(|py| py.allow_threads(|| std::fs::rename(&src_buf, &dst_buf)));
        result.map_err(io_err_to_pyerr)
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Phase 3: Link Creation
// ═══════════════════════════════════════════════════════════════════════

/// Create a symbolic link at ``link`` pointing to ``target``.
///
/// On Windows, ``target_is_directory`` indicates whether the target is a
/// directory (required for correct symlink creation on Windows).
#[cfg(unix)]
pub fn symlink(target: &OsStr, link: &OsStr, target_is_directory: bool) -> PyResult<()> {
    let _ = target_is_directory;
    let target_buf = StdPath::new(target).to_path_buf();
    let link_buf = StdPath::new(link).to_path_buf();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| std::os::unix::fs::symlink(&target_buf, &link_buf))
    });
    result.map_err(io_err_to_pyerr)
}

/// Create a symbolic link at ``link`` pointing to ``target`` (Windows).
#[cfg(not(unix))]
pub fn symlink(target: &OsStr, link: &OsStr, target_is_directory: bool) -> PyResult<()> {
    let target_buf = StdPath::new(target).to_path_buf();
    let link_buf = StdPath::new(link).to_path_buf();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| {
            if target_is_directory {
                std::os::windows::fs::symlink_dir(&target_buf, &link_buf)
            } else {
                std::os::windows::fs::symlink_file(&target_buf, &link_buf)
            }
        })
    });
    result.map_err(io_err_to_pyerr)
}

/// Create a hard link at ``dst`` pointing to ``src``.
pub fn hardlink(src: &OsStr, dst: &OsStr) -> PyResult<()> {
    let src_buf = StdPath::new(src).to_path_buf();
    let dst_buf = StdPath::new(dst).to_path_buf();
    let result = Python::with_gil(|py| py.allow_threads(|| std::fs::hard_link(&src_buf, &dst_buf)));
    result.map_err(io_err_to_pyerr)
}

// ═══════════════════════════════════════════════════════════════════════
// Phase 3: File I/O
// ═══════════════════════════════════════════════════════════════════════

/// Read the entire contents of a file as bytes.
pub fn read_bytes(path: &OsStr) -> PyResult<Vec<u8>> {
    let path_buf = StdPath::new(path).to_path_buf();
    let result = Python::with_gil(|py| py.allow_threads(|| std::fs::read(&path_buf)));
    result.map_err(io_err_to_pyerr)
}

/// Read the entire contents of a file as text, with optional encoding.
pub fn read_text(path: &OsStr, encoding: Option<&str>, errors: Option<&str>) -> PyResult<String> {
    let bytes = read_bytes(path)?;
    let enc = encoding.unwrap_or("utf-8");
    let err_handling = errors.unwrap_or("strict");

    Python::with_gil(|py| {
        let codecs = py.import("codecs")?;
        let decoded =
            codecs.call_method1("decode", (PyBytes::new(py, &bytes), enc, err_handling))?;
        decoded.extract::<String>()
    })
}

/// Write bytes to a file, creating it if it doesn't exist.
pub fn write_bytes(path: &OsStr, data: &[u8]) -> PyResult<()> {
    let path_buf = StdPath::new(path).to_path_buf();
    let data_buf = data.to_vec();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| -> Result<(), io::Error> {
            let mut f = std::fs::File::create(&path_buf)?;
            f.write_all(&data_buf)?;
            Ok(())
        })
    });
    result.map_err(io_err_to_pyerr)
}

/// Write text to a file, encoding with the given encoding.
pub fn write_text(
    path: &OsStr,
    data: &str,
    encoding: Option<&str>,
    errors: Option<&str>,
    newline: Option<&str>,
) -> PyResult<()> {
    let enc = encoding.unwrap_or("utf-8");
    let err_handling = errors.unwrap_or("strict");

    // Encode the text via Python's codecs module for full CPython compatibility.
    let encoded = Python::with_gil(|py| {
        let codecs = py.import("codecs")?;
        let result = codecs.call_method1("encode", (data, enc, err_handling))?;
        result.extract::<Vec<u8>>()
    })?;

    // Apply newline translation if requested
    let final_bytes = if let Some(nl) = newline {
        if nl.is_empty() || nl == "\n" {
            encoded
        } else {
            // Translate \n to the target newline
            let mut result = Vec::new();
            let nl_bytes = nl.as_bytes();
            for &b in &encoded {
                if b == b'\n' {
                    result.extend_from_slice(nl_bytes);
                } else if b == b'\r' {
                    // Skip \r in universal newline mode when nl is set
                    continue;
                } else {
                    result.push(b);
                }
            }
            result
        }
    } else {
        encoded
    };

    write_bytes(path, &final_bytes)
}

// ═══════════════════════════════════════════════════════════════════════
// Phase 3: Directory Traversal
// ═══════════════════════════════════════════════════════════════════════

/// A single directory entry from ``iterdir()``.
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// The full path of the entry.
    pub path: OsString,
    /// The filename component only.
    pub name: OsString,
    /// Whether the entry is a directory.
    pub is_dir: bool,
    /// Whether the entry is a symlink.
    pub is_symlink: bool,
}

/// Iterate over entries in a directory.
///
/// Returns a ``Vec<DirEntry>`` with the directory contents.
/// Entries ``"."`` and ``".."`` are excluded.
pub fn read_dir(path: &OsStr) -> PyResult<Vec<DirEntry>> {
    let path_buf = StdPath::new(path).to_path_buf();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| -> Result<Vec<DirEntry>, io::Error> {
            let mut entries = Vec::new();
            let dir = std::fs::read_dir(&path_buf)?;
            for entry in dir {
                let entry = entry?;
                let name = entry.file_name();
                // Skip "." and ".." (they're included by read_dir on some platforms)
                if name == "." || name == ".." {
                    continue;
                }
                let ft = entry.file_type()?;
                let full_path = entry.path();
                entries.push(DirEntry {
                    path: OsString::from(full_path.as_os_str()),
                    name,
                    is_dir: ft.is_dir(),
                    is_symlink: ft.is_symlink(),
                });
            }
            Ok(entries)
        })
    });
    result.map_err(io_err_to_pyerr)
}

/// A single ``(dirpath, dirnames, filenames)`` walk entry.
type WalkEntry = (OsString, Vec<OsString>, Vec<OsString>);

/// Walk a directory tree, yielding ``(dirpath, dirnames, filenames)`` tuples.
///
/// This function collects all entries immediately (not lazy). The caller
/// is expected to iterate over the results and manage top-down vs bottom-up
/// ordering.
///
/// Parameters
/// ----------
/// path : &OsStr
///     Root directory to walk.
/// topdown : bool
///     If ``True``, yield directories before their contents.
/// follow_symlinks : bool
///     If ``True``, follow symlinks to directories.
pub fn walk_entries(
    path: &OsStr,
    topdown: bool,
    follow_symlinks: bool,
) -> PyResult<Vec<WalkEntry>> {
    let mut results: Vec<WalkEntry> = Vec::new();
    let mut stack: Vec<OsString> = vec![OsString::from(path)];

    while let Some(current) = stack.pop() {
        let entries = match read_dir(&current) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let mut dirnames: Vec<OsString> = Vec::new();
        let mut filenames: Vec<OsString> = Vec::new();

        for entry in &entries {
            if entry.is_dir {
                dirnames.push(entry.name.clone());
            } else {
                filenames.push(entry.name.clone());
            }
        }

        results.push((current.clone(), dirnames.clone(), filenames));

        if !follow_symlinks {
            // Only push non-symlink directories
            for entry in &entries {
                if entry.is_dir && !entry.is_symlink {
                    stack.push(entry.path.clone());
                }
            }
        } else {
            for entry in &entries {
                if entry.is_dir {
                    stack.push(entry.path.clone());
                }
            }
        }
    }

    if topdown {
        // Results are already collected depth-first (stack-based).
        // For top-down, we need to reorder: yield parent before children.
        // Since we used a stack (LIFO), the results are actually in reverse
        // depth-first order. Let's just sort by depth.
        results.sort_by_key(|(p, _, _)| {
            p.as_encoded_bytes()
                .iter()
                .filter(|&&b| b == b'/' || b == b'\\')
                .count()
        });
    } else {
        // Bottom-up: deeper directories first.
        results.sort_by_key(|(p, _, _)| {
            std::cmp::Reverse(
                p.as_encoded_bytes()
                    .iter()
                    .filter(|&&b| b == b'/' || b == b'\\')
                    .count(),
            )
        });
    }

    Ok(results)
}

// ═══════════════════════════════════════════════════════════════════════
// Phase 3: 3.14 File-Tree Operations
// ═══════════════════════════════════════════════════════════════════════

/// Copy a file or directory tree from ``src`` to ``dst``.
///
/// For files: copies file contents and metadata.
/// For directories: recursively copies the entire tree.
/// For symlinks: copies the symlink (not the target), or follows if
/// ``follow_symlinks=True``.
pub fn copy_tree(
    src: &OsStr,
    dst: &OsStr,
    follow_symlinks: bool,
    dirs_exist_ok: bool,
) -> PyResult<()> {
    let src_path = StdPath::new(src);
    let dst_path = StdPath::new(dst);

    // Clear any stale visited-path state from previous copy operations.
    copy_dir_recursive_reset_visited();

    let result = Python::with_gil(|py| {
        py.allow_threads(|| -> Result<(), io::Error> {
            let md = std::fs::symlink_metadata(src_path)?;
            let ft = md.file_type();

            if ft.is_symlink() {
                if follow_symlinks {
                    // Follow the symlink and copy what it points to
                    let target_path = std::fs::read_link(src_path)?;
                    // Resolve relative to src's directory
                    let resolved = if target_path.is_relative() {
                        src_path
                            .parent()
                            .unwrap_or(StdPath::new("."))
                            .join(&target_path)
                    } else {
                        target_path
                    };
                    // Check if resolved target exists
                    let target_md = std::fs::symlink_metadata(&resolved)?;
                    if target_md.is_dir() {
                        copy_dir_recursive(&resolved, dst_path, follow_symlinks, dirs_exist_ok)?;
                    } else {
                        std::fs::create_dir_all(dst_path.parent().unwrap_or(StdPath::new(".")))?;
                        std::fs::copy(&resolved, dst_path)?;
                    }
                } else {
                    // Copy just the symlink
                    let target = std::fs::read_link(src_path)?;
                    if dst_path.exists() {
                        if dst_path.is_symlink() || dst_path.is_file() {
                            std::fs::remove_file(dst_path)?;
                        } else {
                            return Err(io::Error::new(
                                io::ErrorKind::AlreadyExists,
                                format!("'{}' already exists", dst_path.display()),
                            ));
                        }
                    }
                    #[cfg(unix)]
                    std::os::unix::fs::symlink(&target, dst_path)?;
                    #[cfg(windows)]
                    {
                        let target_md = std::fs::symlink_metadata(resolved_target(src_path));
                        let is_dir = target_md.map(|m| m.is_dir()).unwrap_or(false);
                        if is_dir {
                            std::os::windows::fs::symlink_dir(&target, dst_path)?;
                        } else {
                            std::os::windows::fs::symlink_file(&target, dst_path)?;
                        }
                    }
                }
            } else if ft.is_dir() {
                copy_dir_recursive(src_path, dst_path, follow_symlinks, dirs_exist_ok)?;
            } else {
                // Regular file
                std::fs::create_dir_all(dst_path.parent().unwrap_or(StdPath::new(".")))?;
                std::fs::copy(src_path, dst_path)?;
            }
            Ok(())
        })
    });

    result.map_err(io_err_to_pyerr)
}

/// Reset the visited-path set between top-level copy operations.
fn copy_dir_recursive_reset_visited() {
    COPY_VISITED.with(|v| v.borrow_mut().clear());
}

/// Copy a directory recursively.
fn copy_dir_recursive(
    src: &StdPath,
    dst: &StdPath,
    follow_symlinks: bool,
    dirs_exist_ok: bool,
) -> Result<(), io::Error> {
    let src_real = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());

    // Cycle detection: if we've already visited this real path, we have a loop.
    let is_new = COPY_VISITED.with(|v| v.borrow_mut().insert(src_real.clone()));
    if !is_new {
        return Err(io::Error::other(format!(
            "symlink cycle detected while copying '{}'",
            src.display()
        )));
    }

    if dst.exists() {
        if !dirs_exist_ok {
            // Clean up visited entry before returning error.
            COPY_VISITED.with(|v| {
                v.borrow_mut().remove(&src_real);
            });
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("'{}' already exists", dst.display()),
            ));
        }
    } else {
        std::fs::create_dir_all(dst)?;
    }

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let entry_name = entry.file_name();
        if entry_name == "." || entry_name == ".." {
            continue;
        }
        let src_entry = entry.path();
        let dst_entry = dst.join(&entry_name);

        let md = std::fs::symlink_metadata(&src_entry)?;
        if md.file_type().is_symlink() {
            if follow_symlinks {
                let target = std::fs::read_link(&src_entry)?;
                let resolved = if target.is_relative() {
                    src_entry
                        .parent()
                        .unwrap_or(StdPath::new("."))
                        .join(&target)
                } else {
                    target
                };
                let target_md = std::fs::symlink_metadata(&resolved)?;
                if target_md.is_dir() {
                    copy_dir_recursive(&resolved, &dst_entry, true, false)?;
                } else {
                    std::fs::copy(&resolved, &dst_entry)?;
                }
            } else {
                let target = std::fs::read_link(&src_entry)?;
                #[cfg(unix)]
                std::os::unix::fs::symlink(&target, &dst_entry)?;
                #[cfg(windows)]
                {
                    let is_dir = target_md_on_windows(&src_entry, &target);
                    if is_dir {
                        std::os::windows::fs::symlink_dir(&target, &dst_entry)?;
                    } else {
                        std::os::windows::fs::symlink_file(&target, &dst_entry)?;
                    }
                }
            }
        } else if md.is_dir() {
            copy_dir_recursive(&src_entry, &dst_entry, follow_symlinks, false)?;
        } else {
            std::fs::copy(&src_entry, &dst_entry)?;
        }
    }

    COPY_VISITED.with(|v| {
        v.borrow_mut().remove(&src_real);
    });
    Ok(())
}

/// Helper: check if a symlink target is a directory on Windows.
#[cfg(windows)]
fn target_md_on_windows(src_entry: &StdPath, target: &StdPath) -> bool {
    let resolved = if target.is_relative() {
        src_entry.parent().unwrap_or(StdPath::new(".")).join(target)
    } else {
        target.to_path_buf()
    };
    std::fs::symlink_metadata(&resolved)
        .map(|m| m.is_dir())
        .unwrap_or(false)
}

/// Delete a file or directory tree recursively.
///
/// For files and symlinks: removes the entry.
/// For directories: recursively removes the entire tree.
pub fn delete_tree(path: &OsStr, ignore_errors: bool) -> PyResult<()> {
    let path_buf = StdPath::new(path).to_path_buf();
    let result = Python::with_gil(|py| {
        py.allow_threads(|| -> Result<(), io::Error> {
            delete_recursive(path_buf.as_path(), ignore_errors)
        })
    });
    result.map_err(|e| {
        if ignore_errors {
            // If ignore_errors is true, we shouldn't have gotten here
            // because all errors should have been caught. But just in case:
            return pyo3::exceptions::PyOSError::new_err(e.to_string());
        }
        io_err_to_pyerr(e)
    })
}

/// Recursively delete a path.
fn delete_recursive(path: &StdPath, ignore_errors: bool) -> Result<(), io::Error> {
    let md = match std::fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) => {
            if ignore_errors {
                return Ok(());
            }
            return Err(e);
        }
    };

    if md.file_type().is_symlink() {
        // Symlinks are removed without following
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) => {
                if ignore_errors {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    } else if md.is_dir() {
        // Read and delete children
        match std::fs::read_dir(path) {
            Ok(entries) => {
                for entry in entries {
                    match entry {
                        Ok(entry) => {
                            let entry_path = entry.path();
                            let _ = delete_recursive(&entry_path, ignore_errors);
                        }
                        Err(e) => {
                            if !ignore_errors {
                                return Err(e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                if !ignore_errors {
                    return Err(e);
                }
            }
        }
        match std::fs::remove_dir(path) {
            Ok(()) => Ok(()),
            Err(e) => {
                if ignore_errors {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    } else {
        // File, fifo, socket, etc.
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) => {
                if ignore_errors {
                    Ok(())
                } else {
                    Err(e)
                }
            }
        }
    }
}

/// Move a file or directory tree from ``src`` to ``dst``.
///
/// Tries to rename first (same-filesystem fast path). Falls back to
/// copy + delete for cross-filesystem moves.
pub fn move_tree(src: &OsStr, dst: &OsStr) -> PyResult<()> {
    let src_path = StdPath::new(src);
    let dst_path = StdPath::new(dst);

    // Try rename first (fast path for same filesystem)
    if Python::with_gil(|py| py.allow_threads(|| std::fs::rename(src_path, dst_path))).is_ok() {
        return Ok(());
    }

    // Fall back to copy + delete (cross-filesystem)
    // First, determine if src is a file or directory
    let result = Python::with_gil(|py| {
        py.allow_threads(|| -> Result<(), io::Error> {
            let md = std::fs::symlink_metadata(src_path)?;
            if md.file_type().is_symlink() {
                // Copy symlink, then remove original
                let target = std::fs::read_link(src_path)?;
                #[cfg(unix)]
                std::os::unix::fs::symlink(&target, dst_path)?;
                #[cfg(windows)]
                {
                    if md.is_dir() {
                        std::os::windows::fs::symlink_dir(&target, dst_path)?;
                    } else {
                        std::os::windows::fs::symlink_file(&target, dst_path)?;
                    }
                }
                std::fs::remove_file(src_path)?;
            } else if md.is_dir() {
                copy_dir_recursive(src_path, dst_path, false, false)?;
                delete_recursive(src_path, false)?;
            } else {
                std::fs::create_dir_all(dst_path.parent().unwrap_or(StdPath::new(".")))?;
                std::fs::copy(src_path, dst_path)?;
                std::fs::remove_file(src_path)?;
            }
            Ok(())
        })
    });

    result.map_err(io_err_to_pyerr)
}

// Cross-platform helper for symlink target resolving
#[cfg(windows)]
fn resolved_target(src_path: &StdPath) -> std::path::PathBuf {
    if let Ok(target) = std::fs::read_link(src_path) {
        if target.is_relative() {
            src_path.parent().unwrap_or(StdPath::new(".")).join(&target)
        } else {
            target
        }
    } else {
        src_path.to_path_buf()
    }
}
