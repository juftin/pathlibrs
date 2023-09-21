use std::env;
use std::fs;
use std::path::PathBuf;

use pyo3::prelude::pyclass;
use pyo3::prelude::pymethods;
use pyo3::prelude::pymodule;
use pyo3::prelude::PyModule;
use pyo3::prelude::PyResult;
use pyo3::prelude::Python;
use pyo3::Py;
use pyo3::types::{PyDict, PyString, PyTuple, PyType};

/// pathlibrs.Path: a pathlib.Path implementation in Rust
#[pyclass]
pub struct Path {
    #[pyo3(get, set)]
    pub path: PathBuf,
}

#[pymethods]
impl Path {
    /// Construct a PurePath from one or several strings and or existing
    /// PurePath objects.  The strings and path objects are combined so as
    /// to yield a canonicalized path, which is incorporated into the
    /// new PurePath object.
    #[new]
    #[pyo3(signature = (* args, * * kwargs))]
    fn new(args: &PyTuple, kwargs: Option<&PyDict>) -> Self {
        // TODO: support multiple args
        let _ = kwargs;
        // no args = "."
        if args.is_empty() {
            return Path {
                path: PathBuf::from("."),
            };
        }
        // empty string = "."
        let path_str: String = args.get_item(0).unwrap().to_string();
        if path_str.is_empty() {
            return Path {
                path: PathBuf::from("."),
            };
        }
        // otherwise, use the first arg
        Path {
            path: PathBuf::from(path_str),
        }
    }

    /// Return the string representation of the path, suitable for
    /// passing to system calls.
    fn __str__(&self) -> PyResult<String> {
        Ok(self.path.to_str().unwrap().to_string())
    }

    /// Return a string representation of the path
    fn __repr__(&self) -> PyResult<String> {
        let path_str = self.path.to_str().unwrap();
        Ok(format!("Path('{}')", path_str))
    }

    /// The final path component, if any.
    #[getter]
    fn name(&self) -> PyResult<String> {
        Ok(self.path.file_name().unwrap().to_str().unwrap().to_string())
    }

    /// The final component's last suffix, if any.
    ///
    /// This includes the leading period. For example: '.txt'
    #[getter]
    fn suffix(&self) -> PyResult<String> {
        Ok(self.path.extension().unwrap().to_str().unwrap().to_string())
    }

    /// A list of the final component's suffixes, if any.
    ///
    /// These include the leading periods. For example: ['.tar', '.gz']
    #[getter]
    fn suffixes(&self) -> PyResult<Vec<String>> {
        let mut suffixes = Vec::new();
        for suffix in self.path.extension().unwrap().to_str().unwrap().split('.') {
            suffixes.push(suffix.to_string());
        }
        Ok(suffixes)
    }

    #[getter]
    /// The final path component, minus its last suffix.
    fn stem(&self) -> PyResult<String> {
        Ok(self.path.file_stem().unwrap().to_str().unwrap().to_string())
    }

    /// Return a new path with the file name changed.
    fn with_name(&self, name: &PyString) -> PyResult<Path> {
        let mut path = self.path.clone();
        path.set_file_name(name.to_str().unwrap());
        Ok(Path { path })
    }

    /// Return a new path with the stem changed.
    fn with_stem(&self, stem: &PyString) -> PyResult<Path> {
        let mut path = self.path.clone();
        path.set_file_name(stem.to_str().unwrap());
        path.set_extension(self.suffix().unwrap());
        Ok(Path { path })
    }

    /// Return a new path with the file suffix changed.  If the path
    /// has no suffix, add given suffix.  If the given suffix is an empty
    /// string, remove the suffix from the path.
    fn with_suffix(&self, suffix: &PyString) -> PyResult<Path> {
        let mut path = self.path.clone();
        let suffix_str = suffix.to_str().unwrap();
        // raise error if "." not in suffix
        if !suffix_str.contains('.') {
            // show the bad suffix in the error message
            return Err(pyo3::exceptions::PyValueError::new_err(
                format!("Invalid suffix '{}'", suffix_str),
            ));
        }
        path.set_extension(suffix.to_str().unwrap().trim_start_matches('.'));
        Ok(Path { path })
    }

    fn relative_to(&self, _other: &Path) -> PyResult<Path> {
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "Not implemented: `relative_to`",
        ))
    }

    fn is_relative_to(&self, _other: &Path) -> PyResult<bool> {
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "Not implemented: `is_relative_to`",
        ))
    }

    /// An object providing sequence-like access to the
    /// components in the filesystem path.
    #[getter]
    fn parts(&self, py: Python) -> PyResult<Py<PyTuple>> {
        let mut parts = Vec::new();
        for part in self.path.iter() {
            parts.push(part.to_str().unwrap().to_string());
        }
        Ok(PyTuple::new(py, parts).into())
    }

    /// Combine this path with one or several arguments, and return a
    /// new path representing either a subpath (if all arguments are relative
    /// paths) or a totally different path (if one of the arguments is
    /// anchored).
    #[pyo3(signature = (* args))]
    fn joinpath(&self, args: &PyTuple) -> PyResult<Path> {
        let mut path = self.path.clone();
        for arg in args.iter() {
            path.push(arg.to_string());
        }
        Ok(Path { path })
    }

    /// Allow for division-style path joining
    fn __truediv__(&self, key: &PyString) -> PyResult<Path> {
        let mut path = self.path.clone();
        path.push(key.to_string());
        Ok(Path { path })
    }

    /// Allow for division-style path joining
    fn __rtruediv__(&self, key: &PyString) -> PyResult<Path> {
        let mut path = self.path.clone();
        path.push(key.to_string());
        Ok(Path { path })
    }

    /// The logical parent of the path.
    #[getter]
    fn parent(&self) -> PyResult<Path> {
        let path_str = self.path.to_str().unwrap();
        if path_str == "." || path_str == "/" {
            return Ok(Path {
                path: self.path.clone(),
            });
        }
        Ok(Path {
            path: self.path.parent().unwrap().to_path_buf(),
        })
    }

    // /// A sequence of this path's logical parents.
    // #[getter]
    // fn parents(&self, py: Python) -> PyResult<Vec<Path>> {
    //     let mut parents = Vec::new();
    //     let mut path = self.path.clone();
    //     loop {
    //         path = path.parent().unwrap().to_path_buf();
    //         if path.to_str().unwrap() == "." || path.to_str().unwrap() == "/" {
    //             break;
    //         }
    //         parents.push(Path { path });
    //     }
    //     Ok(parents)
    // }

    /// True if the path is absolute (has both a root and, if applicable,
    /// a drive).
    fn is_absolute(&self) -> PyResult<bool> {
        Ok(self.path.is_absolute())
    }

    fn is_reserved(&self) -> PyResult<bool> {
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "Not implemented: `is_reserved`",
        ))
    }

    fn match_(&self, _path_pattern: &PyString) -> PyResult<bool> {
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "Not implemented: `match`",
        ))
    }

    /// Return a new path pointing to the current working directory
    /// (as returned by os.getcwd())
    #[classmethod]
    fn cwd(_cls: &PyType) -> PyResult<Path> {
        // TODO: call os.getcwd() directly for mocking support
        let cwd = env::current_dir().unwrap();
        Ok(Path { path: cwd })
    }

    // Return a new path pointing to the user's home directory (as
    // returned by os.path.expanduser('~')
    #[classmethod]
    fn home(_cls: &PyType) -> PyResult<Path> {
        // TODO: call os.path.expanduser('~') directly for mocking support
        let home = env::var("HOME").unwrap();
        Ok(Path {
            path: PathBuf::from(home),
        })
    }

    /// Return whether other_path is the same or not as this file
    /// (as returned by os.path.samefile())
    fn samefile(&self, other_path: &Path) -> PyResult<bool> {
        Ok(self.path == other_path.path)
    }

    /// Iterate over the files in this directory.  Does not yield any
    /// result for the special paths '.' and '..'.
    fn iterdir(&self) -> PyResult<Vec<Path>> {
        // TODO: Generator vs Vec?
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.path).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.to_str().unwrap() == "." || path.to_str().unwrap() == ".." {
                continue;
            }
            files.push(Path { path });
        }
        Ok(files)
    }

    /// Iterate over this subtree and yield all existing files (of any
    /// kind, including directories) matching the given relative pattern.
    fn glob(&self, pattern: &PyString) -> PyResult<Vec<Path>> {
        let mut files = Vec::new();
        let resolved = self.resolve().unwrap();
        let pattern = format!("{}/{}", resolved.path.to_str().unwrap(), pattern.to_str().unwrap());
        println!("pattern: {}", pattern);
        for entry in glob::glob(&pattern).unwrap() {
            let path = entry.unwrap();
            files.push(Path { path });
        }
        Ok(files)
    }

    /// Recursively yield all existing files (of any kind, including
    /// directories) matching the given relative pattern, anywhere in
    /// this subtree.
    fn rglob(&self, pattern: &PyString) -> PyResult<Vec<Path>> {
        let mut files = Vec::new();
        let resolved = self.resolve().unwrap();
        let pattern = format!("{}/**/{}", resolved.path.to_str().unwrap(), pattern.to_str().unwrap());
        println!("pattern: {}", pattern);
        for entry in glob::glob(&pattern).unwrap() {
            let path = entry.unwrap();
            files.push(Path { path });
        }
        Ok(files)
    }

    /// Make the path absolute, resolving all symlinks on the way and also
    /// normalizing it.
    fn resolve(&self) -> PyResult<Path> {
        if self.path.to_str().unwrap().is_empty() {
            return Err(pyo3::exceptions::PyValueError::new_err(
                "Cannot resolve empty path",
            ));
        }
        Ok(Path {
            path: self.path.canonicalize().unwrap(),
        })
    }

    /// Whether this path exists.
    fn exists(&self) -> PyResult<bool> {
        Ok(self.path.exists())
    }

    /// Whether this path is a directory.
    fn is_dir(&self) -> PyResult<bool> {
        Ok(self.path.is_dir())
    }

    /// Whether this path is a regular file (also True for symlinks pointing
    /// to regular files)
    fn is_file(&self) -> PyResult<bool> {
        Ok(self.path.is_file())
    }

    /// Whether this path is a symbolic link.
    fn is_symlink(&self) -> PyResult<bool> {
        Ok(self
            .path
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink())
    }

    /// Read the file as text and return the contents.
    fn read_text(&self) -> PyResult<String> {
        // TODO: handle options
        let contents = fs::read_to_string(&self.path).unwrap();
        Ok(contents)
    }

    /// Write the string to the file, creating it if it does not exist.
    fn write_text(&self, _contents: &PyString) -> PyResult<()> {
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "Not implemented: `write_text`",
        ))
    }
}

#[pymodule]
fn pathlibrs(_py: Python<'_>, m: &PyModule) -> PyResult<()> {
    m.add_class::<Path>()?;
    Ok(())
}
