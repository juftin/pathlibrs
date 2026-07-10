//! Filesystem operations — stubs for Phase 2+.
//!
//! When implemented, this module will provide:
//!
//! - ``exists()``, ``is_file()``, ``is_dir()``
//! - ``stat()``, ``lstat()``
//! - ``open()``, ``read_text()``, ``write_text()``
//! - ``mkdir()``, ``rmdir()``, ``unlink()``
//! - ``iterdir()``, ``glob()``, ``rglob()``
//! - ``expanduser()``, ``resolve()``, ``symlink_to()``
//!
//! All IO operations will release the GIL during system calls.
