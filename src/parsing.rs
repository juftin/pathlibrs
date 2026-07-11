//! Path parsing — split a raw path into drive, root, and named parts.
//!
//! Both POSIX and Windows flavours are handled in pure Rust, matching
//! CPython 3.12+ pathlib behaviour.

use std::ffi::{OsStr, OsString};

use crate::repr::{ParsedPath, PathFlavour};

/// Parse a raw path string into its components.
pub fn parse_path(path: &OsStr, flavour: PathFlavour) -> ParsedPath {
    match flavour {
        PathFlavour::Posix => parse_posix(path),
        PathFlavour::Windows => parse_windows(path),
    }
}

// ---------------------------------------------------------------------------
// POSIX
// ---------------------------------------------------------------------------

/// Separator byte for POSIX paths.
const POSIX_SEP: u8 = b'/';

/// Parse a POSIX path.
fn parse_posix(path: &OsStr) -> ParsedPath {
    let bytes = path.as_encoded_bytes();

    if bytes.is_empty() {
        return ParsedPath {
            drive: None,
            root: None,
            parts: Vec::new(),
            anchor_length: 0,
            has_name: false,
        };
    }

    let (root, anchor_len) = parse_posix_root(bytes);

    // Split the remainder into parts
    let rest = &bytes[anchor_len..];
    let parts: Vec<OsString> = rest
        .split(|&b| b == POSIX_SEP)
        .filter(|s| !s.is_empty())
        .map(|s| crate::from_os_bytes(s).to_os_string())
        .collect();

    let has_name = !parts.is_empty();

    ParsedPath {
        drive: None,
        root,
        parts,
        anchor_length: anchor_len,
        has_name,
    }
}

/// Extract the POSIX root and its byte length.
///
/// Special-case: exactly 2 leading slashes produce root ``"//"``
/// (POSIX allows implementation-defined semantics for ``//``).
/// One slash or 3+ slashes collapse to ``"/"``.
fn parse_posix_root(bytes: &[u8]) -> (Option<OsString>, usize) {
    if bytes.is_empty() || bytes[0] != POSIX_SEP {
        return (None, 0);
    }

    let leading_slashes = bytes.iter().take_while(|&&b| b == POSIX_SEP).count();

    if leading_slashes == 2 {
        // Exactly 2 slashes → root is "//"
        // But only if there are actually exactly 2 slashes at the start,
        // with the third character being non-slash or end-of-string.
        // If we have exactly 2 bytes and both are '/', root is "//".
        // If we have more and the third is not '/', root is "//".
        (Some(crate::from_os_bytes(&bytes[..2]).to_os_string()), 2)
    } else {
        // 1 or 3+ slashes → root is "/"
        (Some(crate::from_os_bytes(b"/").to_os_string()), 1)
    }
}

// ---------------------------------------------------------------------------
// Windows
// ---------------------------------------------------------------------------

/// Parse a Windows-style path.
///
/// Recognises drive letters, UNC shares, device paths, and
/// extended-length prefixes. Both ``\\`` and ``/`` are treated
/// as separators.
fn parse_windows(path: &OsStr) -> ParsedPath {
    let raw = path.as_encoded_bytes();

    if raw.is_empty() {
        return ParsedPath {
            drive: None,
            root: None,
            parts: Vec::new(),
            anchor_length: 0,
            has_name: false,
        };
    }

    // Normalise forward slashes to backslashes so that parsed components
    // (drive, root, parts) always use backslash separators regardless of
    // whether the input used ``/`` or ``\\``.
    let normalised: Vec<u8> = raw
        .iter()
        .map(|&b| if b == b'/' { b'\\' } else { b })
        .collect();

    let (drive, root, anchor_len) = parse_windows_drive_root(&normalised);

    // Split the remainder into parts (treating both \ and / as separators)
    let rest = &normalised[anchor_len..];
    let parts: Vec<OsString> = split_windows_parts(rest);

    let has_name = !parts.is_empty();

    ParsedPath {
        drive,
        root,
        parts,
        anchor_length: anchor_len,
        has_name,
    }
}

/// Return true if `b` is a Windows path separator.
#[inline]
fn is_win_sep(b: u8) -> bool {
    b == b'\\' || b == b'/'
}

/// Return true if `b` is an ASCII alphabetic character.
#[inline]
fn is_alpha(b: u8) -> bool {
    b.is_ascii_alphabetic()
}

/// Parse the drive + root anchor from a Windows path.
///
/// Returns `(drive, root, anchor_length)`.
fn parse_windows_drive_root(bytes: &[u8]) -> (Option<OsString>, Option<OsString>, usize) {
    let len = bytes.len();

    // ── Extended-length prefix: \\?\  or  \\.\  ────────────────────────
    if len >= 4
        && is_win_sep(bytes[0])
        && is_win_sep(bytes[1])
        && (bytes[2] == b'?' || bytes[2] == b'.')
        && is_win_sep(bytes[3])
    {
        // \\?\C:\...  or  \\?\UNC\server\share\...  or  \\.\C:\...
        let prefix = &bytes[..4];

        // After the prefix, find the drive or UNC root.
        let remaining = &bytes[4..];
        let remaining_len = remaining.len();

        // \\?\UNC  — everything after "UNC" follows the same server\share
        // pattern as normal UNC but with the \\?\UNC prefix as the drive base.
        if prefix[2] == b'?'
            && remaining_len >= 3
            && remaining[0] == b'U'
            && remaining[1] == b'N'
            && remaining[2] == b'C'
        {
            // Skip "UNC" and optional trailing separator
            let after_unc = if remaining_len > 3 && is_win_sep(remaining[3]) {
                &remaining[4..] // skip "UNC\"
            } else {
                // \\?\UNC (just the literal, no trailing separator)
                let mut drive =
                    Vec::with_capacity(4 + 3);
                drive.extend_from_slice(prefix);
                drive.extend_from_slice(b"UNC");
                return (
                    Some(crate::from_os_bytes(&drive).to_os_string()),
                    None,
                    len.min(4 + 3),
                );
            };

            if after_unc.is_empty() {
                // \\?\UNC\  — drive includes trailing separator
                let mut drive = Vec::with_capacity(4 + 4);
                drive.extend_from_slice(prefix);
                drive.extend_from_slice(b"UNC\\");
                return (
                    Some(crate::from_os_bytes(&drive).to_os_string()),
                    None,
                    len,
                );
            }

            // Find server name (up to next separator or end)
            match after_unc.iter().position(|&b| is_win_sep(b)) {
                Some(sep_pos) => {
                    let server = &after_unc[..sep_pos];
                    let after_server = &after_unc[sep_pos + 1..];

                    if after_server.is_empty() {
                        // \\?\UNC\server\ — drive includes trailing separator
                        let mut drive = Vec::with_capacity(4 + 3 + 1 + server.len() + 1);
                        drive.extend_from_slice(prefix);
                        drive.extend_from_slice(b"UNC\\");
                        drive.extend_from_slice(server);
                        drive.push(b'\\');
                        return (
                            Some(crate::from_os_bytes(&drive).to_os_string()),
                            None,
                            len,
                        );
                    }

                    // Find share (up to next separator or end)
                    match after_server.iter().position(|&b| is_win_sep(b)) {
                        Some(share_sep) => {
                            let share = &after_server[..share_sep];
                            let anchor_end =
                                4 + 3 + 1 + server.len() + 1 + share_sep + 1;
                            let mut drive = Vec::with_capacity(
                                4 + 3 + 1 + server.len() + 1 + share.len(),
                            );
                            drive.extend_from_slice(prefix);
                            drive.extend_from_slice(b"UNC\\");
                            drive.extend_from_slice(server);
                            drive.push(b'\\');
                            drive.extend_from_slice(share);
                            return (
                                Some(crate::from_os_bytes(&drive).to_os_string()),
                                Some(crate::from_os_bytes(b"\\").to_os_string()),
                                anchor_end.min(len),
                            );
                        }
                        None => {
                            // \\?\UNC\server\share — share with no trailing sep
                            let share = after_server;
                            let anchor_end = len;
                            let mut drive = Vec::with_capacity(
                                4 + 3 + 1 + server.len() + 1 + share.len(),
                            );
                            drive.extend_from_slice(prefix);
                            drive.extend_from_slice(b"UNC\\");
                            drive.extend_from_slice(server);
                            drive.push(b'\\');
                            drive.extend_from_slice(share);
                            return (
                                Some(crate::from_os_bytes(&drive).to_os_string()),
                                Some(crate::from_os_bytes(b"\\").to_os_string()),
                                anchor_end,
                            );
                        }
                    }
                }
                None => {
                    // \\?\UNC\server — no trailing separator, no root
                    let server = after_unc;
                    let mut drive =
                        Vec::with_capacity(4 + 3 + 1 + server.len());
                    drive.extend_from_slice(prefix);
                    drive.extend_from_slice(b"UNC\\");
                    drive.extend_from_slice(server);
                    return (
                        Some(crate::from_os_bytes(&drive).to_os_string()),
                        None,
                        len,
                    );
                }
            }
        }

        // \\?\C:\  or  \\.\C:\  — drive letter after prefix

        // \\?\C:\  or  \\.\C:\  — drive letter after prefix
        if remaining_len >= 2 && is_alpha(remaining[0]) && remaining[1] == b':' {
            let drive_end = 4 + 2; // prefix + "C:"
            let has_root = remaining_len > 2 && is_win_sep(remaining[2]);
            let anchor_end = if has_root { drive_end + 1 } else { drive_end };

            return (
                Some(crate::from_os_bytes(&bytes[..drive_end]).to_os_string()),
                if has_root {
                    Some(crate::from_os_bytes(b"\\").to_os_string())
                } else {
                    None
                },
                anchor_end.min(len),
            );
        }

        // Extended prefix without drive letter or UNC — everything after the
        // prefix is the device name and belongs to the drive (e.g.
        // \\.\BootPartition\, \\.\PhysicalDrive0, \\?\Volume{}\).
        if remaining_len > 0 {
            let has_trailing_sep =
                is_win_sep(remaining[remaining_len - 1]);
            let drive_body = if has_trailing_sep {
                &remaining[..remaining_len - 1]
            } else {
                remaining
            };
            let mut drive = Vec::with_capacity(4 + drive_body.len());
            drive.extend_from_slice(prefix);
            drive.extend_from_slice(drive_body);
            return (
                Some(crate::from_os_bytes(&drive).to_os_string()),
                if has_trailing_sep {
                    Some(crate::from_os_bytes(b"\\").to_os_string())
                } else {
                    None
                },
                len,
            );
        }
        // Extended prefix with nothing after it (e.g. \\?\ or \\.\)
        return (
            Some(crate::from_os_bytes(prefix).to_os_string()),
            None,
            4.min(len),
        );
    }

    // ── UNC path: \\server\share  ──────────────────────────────────────
    if len >= 2 && is_win_sep(bytes[0]) && is_win_sep(bytes[1]) {
        let after_slashes = &bytes[2..];

        if after_slashes.is_empty() {
            // Just \\ (exactly two slashes) — drive only, no root
            return (
                Some(crate::from_os_bytes(b"\\\\").to_os_string()),
                None,
                2,
            );
        }

        // Find the server name (up to next separator or end)
        match after_slashes.iter().position(|&b| is_win_sep(b)) {
            Some(sep_pos) => {
                // Server found, followed by separator
                let server = &after_slashes[..sep_pos];
                let after_server = &after_slashes[sep_pos + 1..]; // skip separator

                if after_server.is_empty() {
                    // \\server\ — drive includes trailing separator, no root
                    let mut drive = Vec::with_capacity(2 + server.len() + 1);
                    drive.extend_from_slice(b"\\\\");
                    drive.extend_from_slice(server);
                    drive.push(b'\\');
                    return (
                        Some(crate::from_os_bytes(&drive).to_os_string()),
                        None,
                        len,
                    );
                }

                // Find the share (up to next separator or end)
                match after_server.iter().position(|&b| is_win_sep(b)) {
                    Some(share_sep) => {
                        // \\server\share\...  — share with trailing separator → root
                        let share = &after_server[..share_sep];
                        let anchor_end = 2 + sep_pos + 1 + share_sep + 1;

                        let mut drive =
                            Vec::with_capacity(2 + server.len() + 1 + share.len());
                        drive.extend_from_slice(b"\\\\");
                        drive.extend_from_slice(server);
                        drive.push(b'\\');
                        drive.extend_from_slice(share);

                        return (
                            Some(crate::from_os_bytes(&drive).to_os_string()),
                            Some(crate::from_os_bytes(b"\\").to_os_string()),
                            anchor_end.min(len),
                        );
                    }
                    None => {
                        // \\server\share — share with no trailing separator
                        // The root is implied.
                        let share = after_server;
                        let anchor_end = len;

                        let mut drive =
                            Vec::with_capacity(2 + server.len() + 1 + share.len());
                        drive.extend_from_slice(b"\\\\");
                        drive.extend_from_slice(server);
                        drive.push(b'\\');
                        drive.extend_from_slice(share);

                        return (
                            Some(crate::from_os_bytes(&drive).to_os_string()),
                            Some(crate::from_os_bytes(b"\\").to_os_string()),
                            anchor_end,
                        );
                    }
                }
            }
            None => {
                // \\server — no separator after server, no root
                let mut drive = Vec::with_capacity(2 + after_slashes.len());
                drive.extend_from_slice(b"\\\\");
                drive.extend_from_slice(after_slashes);
                return (
                    Some(crate::from_os_bytes(&drive).to_os_string()),
                    None,
                    len,
                );
            }
        }
    }

    // ── Drive letter: C: or C:\  ───────────────────────────────────────
    if len >= 2 && is_alpha(bytes[0]) && bytes[1] == b':' {
        let has_root = len > 2 && is_win_sep(bytes[2]);
        let anchor_end = if has_root { 3 } else { 2 };

        return (
            Some(crate::from_os_bytes(&bytes[..2]).to_os_string()),
            if has_root {
                Some(crate::from_os_bytes(b"\\").to_os_string())
            } else {
                None
            },
            anchor_end.min(len),
        );
    }

    // ── Root-only: \ or /  ─────────────────────────────────────────────
    if !bytes.is_empty() && is_win_sep(bytes[0]) {
        return (None, Some(crate::from_os_bytes(b"\\").to_os_string()), 1);
    }

    // ── Relative path, no anchor  ──────────────────────────────────────
    (None, None, 0)
}

/// Split a byte slice into Windows path parts (on ``\\`` or ``/``).
fn split_windows_parts(bytes: &[u8]) -> Vec<OsString> {
    let mut parts: Vec<OsString> = Vec::new();
    let mut start = 0usize;

    for (i, &b) in bytes.iter().enumerate() {
        if is_win_sep(b) {
            if i > start {
                parts.push(crate::from_os_bytes(&bytes[start..i]).to_os_string());
            }
            start = i + 1;
        }
    }

    if start < bytes.len() {
        parts.push(crate::from_os_bytes(&bytes[start..]).to_os_string());
    }

    parts
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    // -- POSIX ----------------------------------------------------------

    #[test]
    fn test_posix_empty() {
        let p = parse_path(OsStr::new(""), PathFlavour::Posix);
        assert_eq!(p.drive, None);
        assert_eq!(p.root, None);
        assert!(p.parts.is_empty());
        assert!(!p.has_name);
    }

    #[test]
    fn test_posix_absolute() {
        let p = parse_path(OsStr::new("/foo/bar"), PathFlavour::Posix);
        assert_eq!(p.root.as_deref(), Some(OsStr::new("/")));
        assert_eq!(p.parts.len(), 2);
        assert_eq!(p.parts[0], "foo");
        assert_eq!(p.parts[1], "bar");
        assert!(p.has_name);
    }

    #[test]
    fn test_posix_root_only() {
        let p = parse_path(OsStr::new("/"), PathFlavour::Posix);
        assert_eq!(p.root.as_deref(), Some(OsStr::new("/")));
        assert!(p.parts.is_empty());
        assert!(!p.has_name);
    }

    #[test]
    fn test_posix_relative() {
        let p = parse_path(OsStr::new("foo/bar"), PathFlavour::Posix);
        assert_eq!(p.root, None);
        assert_eq!(p.parts.len(), 2);
    }

    #[test]
    fn test_posix_double_slash_root() {
        // Exactly 2 leading slashes: special POSIX behaviour
        let p = parse_path(OsStr::new("//foo/bar"), PathFlavour::Posix);
        assert_eq!(p.root.as_deref(), Some(OsStr::new("//")));
        assert_eq!(p.anchor_length, 2);
        assert_eq!(p.parts, &["foo", "bar"]);
    }

    #[test]
    fn test_posix_triple_slash_root() {
        let p = parse_path(OsStr::new("///foo"), PathFlavour::Posix);
        assert_eq!(p.root.as_deref(), Some(OsStr::new("/")));
        assert_eq!(p.anchor_length, 1);
        assert_eq!(p.parts, &["foo"]);
    }

    #[test]
    fn test_posix_dot_components() {
        let p = parse_path(OsStr::new("/foo/./bar/../baz"), PathFlavour::Posix);
        assert_eq!(p.parts, &["foo", ".", "bar", "..", "baz"]);
    }

    // -- Windows --------------------------------------------------------

    fn win(s: &str) -> OsString {
        OsString::from(s)
    }

    #[test]
    fn test_windows_empty() {
        let p = parse_path(OsStr::new(""), PathFlavour::Windows);
        assert_eq!(p.drive, None);
        assert_eq!(p.root, None);
        assert!(p.parts.is_empty());
    }

    #[test]
    fn test_windows_drive_letter_absolute() {
        let p = parse_path(&win("C:\\foo\\bar"), PathFlavour::Windows);
        assert_eq!(p.drive.as_deref(), Some(OsStr::new("C:")));
        assert_eq!(p.root.as_deref(), Some(OsStr::new("\\")));
        assert_eq!(p.parts, &["foo", "bar"]);
        assert_eq!(p.anchor_length, 3); // "C:\"
        assert!(p.has_name);
    }

    #[test]
    fn test_windows_drive_letter_relative() {
        let p = parse_path(&win("C:foo"), PathFlavour::Windows);
        assert_eq!(p.drive.as_deref(), Some(OsStr::new("C:")));
        assert_eq!(p.root, None);
        assert_eq!(p.parts, &["foo"]);
        assert_eq!(p.anchor_length, 2); // "C:"
    }

    #[test]
    fn test_windows_unc() {
        let p = parse_path(&win("\\\\server\\share\\foo"), PathFlavour::Windows);
        assert_eq!(p.drive.as_deref(), Some(OsStr::new("\\\\server\\share")));
        assert_eq!(p.root.as_deref(), Some(OsStr::new("\\")));
        assert_eq!(p.parts, &["foo"]);
    }

    #[test]
    fn test_windows_forward_slashes() {
        let p = parse_path(&win("C:/foo/bar"), PathFlavour::Windows);
        assert_eq!(p.drive.as_deref(), Some(OsStr::new("C:")));
        assert_eq!(p.root.as_deref(), Some(OsStr::new("\\")));
        assert_eq!(p.parts, &["foo", "bar"]);
    }

    #[test]
    fn test_windows_root_only() {
        let p = parse_path(&win("\\"), PathFlavour::Windows);
        assert_eq!(p.drive, None);
        assert_eq!(p.root.as_deref(), Some(OsStr::new("\\")));
        assert!(p.parts.is_empty());
    }

    #[test]
    fn test_windows_extended_path() {
        let p = parse_path(&win("\\\\?\\C:\\foo"), PathFlavour::Windows);
        assert_eq!(p.drive.as_deref(), Some(OsStr::new("\\\\?\\C:")));
        assert_eq!(p.root.as_deref(), Some(OsStr::new("\\")));
        assert_eq!(p.parts, &["foo"]);
    }

    #[test]
    fn test_windows_extended_unc() {
        let p = parse_path(&win("\\\\?\\UNC\\server\\share\\foo"), PathFlavour::Windows);
        assert_eq!(
            p.drive.as_deref(),
            Some(OsStr::new("\\\\?\\UNC\\server\\share"))
        );
        assert_eq!(p.root.as_deref(), Some(OsStr::new("\\")));
        assert_eq!(p.parts, &["foo"]);
    }

    #[test]
    fn test_windows_device_path() {
        let p = parse_path(&win("\\\\.\\C:\\foo"), PathFlavour::Windows);
        assert_eq!(p.drive.as_deref(), Some(OsStr::new("\\\\.\\C:")));
        assert_eq!(p.root.as_deref(), Some(OsStr::new("\\")));
        assert_eq!(p.parts, &["foo"]);
    }

    #[test]
    fn test_windows_relative_no_drive() {
        let p = parse_path(&win("foo\\bar"), PathFlavour::Windows);
        assert_eq!(p.drive, None);
        assert_eq!(p.root, None);
        assert_eq!(p.parts, &["foo", "bar"]);
    }
}
