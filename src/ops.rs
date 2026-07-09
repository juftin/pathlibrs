//! Zero-copy path operations — ``name``, ``stem``, ``suffix`` on ``&OsStr``.
//!
//! These work directly on byte slices to avoid allocating new strings.

use std::ffi::{OsStr, OsString};
use std::os::unix::ffi::OsStrExt;

/// Separator predicate for the given flavour.
#[inline]
pub fn is_sep(b: u8, is_windows: bool) -> bool {
    if is_windows {
        b == b'\\' || b == b'/'
    } else {
        b == b'/'
    }
}

/// Extract the **final path component** (the "name") from a byte slice.
///
/// Returns [`None`] if there is no name (e.g. the path ends at the root
/// or is empty after the anchor).
///
/// The `anchor_end` is the byte offset where the anchor (drive+root) ends.
pub fn name_from_bytes(bytes: &[u8], anchor_end: usize, is_windows: bool) -> Option<&OsStr> {
    let tail = &bytes[anchor_end..];

    // Strip trailing separators (e.g. "foo/" → "foo")
    let end = trim_trailing_seps(tail, is_windows);
    let tail = &tail[..end];

    if tail.is_empty() {
        return None;
    }

    // Find the last separator
    let last_sep = tail.iter().rposition(|&b| is_sep(b, is_windows));
    let start = match last_sep {
        Some(pos) => pos + 1,
        None => 0,
    };

    let name_bytes = &tail[start..];
    if name_bytes.is_empty() {
        None
    } else {
        Some(OsStr::from_bytes(name_bytes))
    }
}

/// Extract the **final suffix** (last ``.ext``) from a name byte slice.
///
/// Returns [`None`] if there is no suffix. A leading dot (``".bashrc"``)
/// does NOT count as a suffix.
pub fn suffix_from_name(name: &OsStr) -> Option<&OsStr> {
    let name_bytes = name.as_bytes();
    if name_bytes.is_empty() || name_bytes == b"." || name_bytes == b".." {
        return None;
    }

    // Find the last dot that is NOT the first character
    let dot_pos = name_bytes[1..].iter().rposition(|&b| b == b'.');
    match dot_pos {
        Some(pos) => {
            let actual_pos = pos + 1; // adjust for [1..] offset
            Some(OsStr::from_bytes(&name_bytes[actual_pos..]))
        }
        None => None,
    }
}

/// Extract all suffixes from a name (e.g. ``".tar.gz"`` → ``[".tar", ".gz"]``).
pub fn suffixes_from_name(name: &OsStr) -> Vec<OsString> {
    let name_bytes = name.as_bytes();
    let mut result = Vec::new();

    if name_bytes.len() <= 1 || name_bytes == b".." {
        return result;
    }

    // Find all dot positions starting from index 1
    let mut dot_positions: Vec<usize> = Vec::new();
    let mut search_start = 1;
    while search_start < name_bytes.len() {
        if let Some(dot_pos) = name_bytes[search_start..].iter().position(|&b| b == b'.') {
            let actual_pos = search_start + dot_pos;
            dot_positions.push(actual_pos);
            search_start = actual_pos + 1;
        } else {
            break;
        }
    }

    // Each suffix runs from a dot to the NEXT dot (or end of name)
    for i in 0..dot_positions.len() {
        let start = dot_positions[i];
        let end = if i + 1 < dot_positions.len() {
            dot_positions[i + 1]
        } else {
            name_bytes.len()
        };
        result.push(OsStr::from_bytes(&name_bytes[start..end]).to_os_string());
    }

    result
}

/// Extract the **stem** from a name (name without the final suffix).
///
/// For ``"foo.tar.gz"``, returns ``Some("foo.tar")``.
/// For ``".bashrc"``, returns ``Some(".bashrc")`` (leading dot is not a suffix).
pub fn stem_from_name(name: &OsStr) -> Option<&OsStr> {
    let name_bytes = name.as_bytes();
    if name_bytes.is_empty() || name_bytes == b"." || name_bytes == b".." {
        return Some(name);
    }

    match suffix_from_name(name) {
        Some(suffix) => {
            let suffix_len = suffix.as_bytes().len();
            let stem_end = name_bytes.len() - suffix_len;
            if stem_end == 0 {
                // Name was just the suffix — shouldn't happen with our suffix logic,
                // but handle gracefully.
                Some(name)
            } else {
                Some(OsStr::from_bytes(&name_bytes[..stem_end]))
            }
        }
        None => Some(name),
    }
}

/// Check if a byte slice is empty or contains only separators.
pub fn is_empty_path(bytes: &[u8], is_windows: bool) -> bool {
    bytes.iter().all(|&b| is_sep(b, is_windows))
}

/// Strip trailing separator bytes from a byte slice.
/// Returns the new length.
pub fn trim_trailing_seps(bytes: &[u8], is_windows: bool) -> usize {
    let mut end = bytes.len();
    while end > 0 && is_sep(bytes[end - 1], is_windows) {
        end -= 1;
    }
    end
}

/// Find the byte offset of the last separator in `bytes`.
pub fn last_sep_offset(bytes: &[u8], is_windows: bool) -> Option<usize> {
    bytes.iter().rposition(|&b| is_sep(b, is_windows))
}

/// Split a path bytes after the anchor into its parent prefix and name.
///
/// Returns `(parent_end, name_start)` relative to `anchor_end`.
/// `parent_end` is the byte offset after anchor where the parent portion ends
/// (excluding trailing separator). `name_start` is where the name begins.
pub fn split_parent_name(
    bytes: &[u8],
    anchor_end: usize,
    is_windows: bool,
) -> Option<(usize, Option<usize>)> {
    let tail = &bytes[anchor_end..];
    let end = trim_trailing_seps(tail, is_windows);
    let tail = &tail[..end];

    if tail.is_empty() {
        return None; // only anchor, no parts — no parent
    }

    let last_sep = tail.iter().rposition(|&b| is_sep(b, is_windows));
    match last_sep {
        Some(pos) => {
            let parent_end = anchor_end + pos;
            let name_start = anchor_end + pos + 1;
            Some((parent_end, Some(name_start)))
        }
        None => {
            // No separator in tail — the parent is the anchor only
            Some((anchor_end, Some(anchor_end)))
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_posix() {
        assert_eq!(
            name_from_bytes(b"/foo/bar.txt", 1, false).map(|s| s.as_bytes()),
            Some(&b"bar.txt"[..])
        );
        assert_eq!(name_from_bytes(b"/", 1, false), None);
        assert_eq!(
            name_from_bytes(b"foo.txt", 0, false).map(|s| s.as_bytes()),
            Some(&b"foo.txt"[..])
        );
    }

    #[test]
    fn test_name_windows() {
        assert_eq!(
            name_from_bytes(b"C:\\foo\\bar.txt", 3, true).map(|s| s.as_bytes()),
            Some(&b"bar.txt"[..])
        );
        assert_eq!(name_from_bytes(b"C:\\", 3, true), None);
        assert_eq!(
            name_from_bytes(b"C:foo.txt", 2, true).map(|s| s.as_bytes()),
            Some(&b"foo.txt"[..])
        );
    }

    #[test]
    fn test_suffix() {
        assert_eq!(
            suffix_from_name(OsStr::new("bar.txt")).map(|s| s.as_bytes()),
            Some(&b".txt"[..])
        );
        assert_eq!(
            suffix_from_name(OsStr::new("foo.tar.gz")).map(|s| s.as_bytes()),
            Some(&b".gz"[..])
        );
        assert_eq!(suffix_from_name(OsStr::new(".bashrc")), None);
        assert_eq!(suffix_from_name(OsStr::new("Makefile")), None);
        assert_eq!(suffix_from_name(OsStr::new(".")), None);
        assert_eq!(suffix_from_name(OsStr::new("..")), None);
    }

    #[test]
    fn test_suffixes() {
        let s: Vec<String> = suffixes_from_name(OsStr::new("foo.tar.gz"))
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert_eq!(s, vec![".tar", ".gz"]);

        let s: Vec<String> = suffixes_from_name(OsStr::new(".bashrc"))
            .iter()
            .map(|s| s.to_string_lossy().into_owned())
            .collect();
        assert!(s.is_empty());
    }

    #[test]
    fn test_stem() {
        assert_eq!(
            stem_from_name(OsStr::new("bar.txt")).map(|s| s.as_bytes()),
            Some(&b"bar"[..])
        );
        assert_eq!(
            stem_from_name(OsStr::new("foo.tar.gz")).map(|s| s.as_bytes()),
            Some(&b"foo.tar"[..])
        );
        assert_eq!(
            stem_from_name(OsStr::new(".bashrc")).map(|s| s.as_bytes()),
            Some(&b".bashrc"[..])
        );
        assert_eq!(
            stem_from_name(OsStr::new("Makefile")).map(|s| s.as_bytes()),
            Some(&b"Makefile"[..])
        );
    }

    #[test]
    fn test_trim_trailing_seps() {
        assert_eq!(trim_trailing_seps(b"foo/", false), 3);
        assert_eq!(trim_trailing_seps(b"foo//", false), 3);
        assert_eq!(trim_trailing_seps(b"foo\\", true), 3);
        assert_eq!(trim_trailing_seps(b"foo/\\", true), 3);
    }
}
