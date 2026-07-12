//! Glob pattern matching (fnmatch-style) for path ``.match()`` and ``.full_match()``.
//!
//! Implements the subset of fnmatch used by CPython's pathlib:
//! ``*``, ``?``, ``[seq]``, ``[!seq]``.

use std::ffi::OsStr;

/// Compiled glob pattern for fast repeated matching.
#[derive(Debug, Clone)]
pub struct GlobPattern {
    /// Pattern segments (split on ``/``).
    segments: Vec<Vec<u8>>,
    /// Whether the pattern is absolute (starts with ``/``).
    is_absolute: bool,
    /// Whether the pattern is case-sensitive.
    case_sensitive: bool,
}

impl GlobPattern {
    /// Compile a fnmatch-style pattern string.
    pub fn new(pattern: &OsStr, case_sensitive: bool) -> Self {
        let bytes = pattern.as_encoded_bytes();
        let is_absolute = bytes.first().is_some_and(|&b| b == b'/');

        let segments: Vec<Vec<u8>> = if bytes.is_empty() {
            vec![vec![]]
        } else {
            bytes.split(|&b| b == b'/').map(|s| s.to_vec()).collect()
        };

        Self {
            segments,
            is_absolute,
            case_sensitive,
        }
    }

    /// Match this pattern against a path string.
    ///
    /// Returns `true` if the path matches the pattern.
    /// For relative patterns, the pattern is matched against the tail of the path.
    pub fn matches(&self, path: &OsStr) -> bool {
        let path_bytes = path.as_encoded_bytes();

        // If pattern is relative, match against the tail of the path
        if !self.is_absolute {
            // Try matching at every position (right-anchored)
            return self.match_relative(path_bytes);
        }

        // Absolute pattern — must match from the start (after any root/drive)
        // For simplicity, just split the path and match segment-by-segment
        self.match_absolute(path_bytes)
    }

    /// Match this pattern against the *entire* path string.
    ///
    /// Unlike [`matches`], a relative pattern like ``"*.py"`` will NOT match
    /// ``"/a/b/foo.py"`` — the segment counts must match exactly.
    pub fn full_matches(&self, path: &OsStr) -> bool {
        let path_bytes = path.as_encoded_bytes();
        let path_segments: Vec<&[u8]> = split_path_segments(path_bytes);

        // For full_match, compare non-empty segments only.
        // The leading empty segment from an absolute path's leading "/"
        // is filtered out so it doesn't count against the segment count.
        let pattern_parts: Vec<&[u8]> = self
            .segments
            .iter()
            .filter(|s| !s.is_empty())
            .map(|s| s.as_slice())
            .collect();
        let path_parts: Vec<&[u8]> = path_segments
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect();

        if pattern_parts.len() != path_parts.len() {
            return false;
        }

        if pattern_parts.is_empty() {
            return path_parts.is_empty();
        }

        // Match each segment with fnmatch — patterns like "*" must match
        // any single segment, not just literal bytes.
        for i in 0..pattern_parts.len() {
            if !fnmatch_bytes(pattern_parts[i], path_parts[i], self.case_sensitive) {
                return false;
            }
        }
        true
    }

    /// Match an absolute pattern against a path.
    fn match_absolute(&self, path: &[u8]) -> bool {
        let path_segments: Vec<&[u8]> = if path.is_empty() {
            vec![&[]]
        } else {
            path.split(|&b| b == b'/').collect()
        };

        self.match_segments(&path_segments)
    }

    /// Match a relative pattern — try anchored at the end.
    fn match_relative(&self, path: &[u8]) -> bool {
        let path_segments: Vec<&[u8]> = if path.is_empty() {
            vec![&[]]
        } else {
            path.split(|&b| b == b'/').collect()
        };

        // Try matching the pattern at the end of the path segments
        if self.segments.len() > path_segments.len() {
            return false;
        }

        let start = path_segments.len() - self.segments.len();
        let tail = &path_segments[start..];
        self.match_segments(tail)
    }

    /// Match pattern segments against path segments.
    ///
    /// Every segment is matched with fnmatch so that glob patterns
    /// like ``//*/*/*.py`` correctly match UNC paths (``//server/share/file.py``).
    /// Because segments are already split on ``/``, the ``*`` wildcard
    /// cannot cross directory boundaries.
    fn match_segments(&self, path_segments: &[&[u8]]) -> bool {
        if self.segments.len() != path_segments.len() {
            return false;
        }
        if self.segments.is_empty() {
            return path_segments.is_empty();
        }
        for (pat, seg) in self.segments.iter().zip(path_segments.iter()) {
            if !fnmatch_bytes(pat, seg, self.case_sensitive) {
                return false;
            }
        }
        true
    }
}

/// fnmatch a single path component (no ``/`` separators).
///
/// Supports ``*``, ``?``, ``[seq]``, and ``[!seq]``.
pub fn fnmatch_bytes(pattern: &[u8], name: &[u8], case_sensitive: bool) -> bool {
    let mut pi = 0usize; // pattern index
    let mut ni = 0usize; // name index
    let mut star_pi: Option<usize> = None;
    let mut star_ni: Option<usize> = None;

    while ni < name.len() || pi < pattern.len() {
        if pi < pattern.len() {
            match pattern[pi] {
                b'?' => {
                    // Match any single character except '/'
                    if ni < name.len() && name[ni] != b'/' {
                        pi += 1;
                        ni += 1;
                        continue;
                    }
                }
                b'*' => {
                    // Match zero or more characters (non-greedy backtracking)
                    star_pi = Some(pi);
                    star_ni = Some(ni);
                    pi += 1;
                    continue;
                }
                b'[' if ni < name.len() => {
                    // Character class: [abc] or [!abc]
                    let class_end = match pattern[pi..].iter().position(|&b| b == b']') {
                        Some(pos) => pi + pos,
                        None => {
                            // Unclosed bracket — treat literally
                            if bytes_match_one(pattern[pi], name[ni], case_sensitive) {
                                pi += 1;
                                ni += 1;
                                continue;
                            } else {
                                break;
                            }
                        }
                    };

                    let class_body = &pattern[pi + 1..class_end];
                    let negated = class_body.first() == Some(&b'!');
                    let chars = if negated {
                        &class_body[1..]
                    } else {
                        class_body
                    };

                    let matched = chars
                        .iter()
                        .any(|&c| bytes_match_one(c, name[ni], case_sensitive));
                    if (matched && !negated) || (!matched && negated) {
                        pi = class_end + 1;
                        ni += 1;
                        continue;
                    }
                }
                _ => {}
            }
        }

        // Literal match fallback or star backtracking
        if pi < pattern.len()
            && ni < name.len()
            && bytes_match_one(pattern[pi], name[ni], case_sensitive)
        {
            pi += 1;
            ni += 1;
            continue;
        }

        // Backtrack on '*'
        if let (Some(sp), Some(sn)) = (star_pi, star_ni) {
            // Consume one more character from the name
            if sn < name.len() {
                pi = sp + 1; // after the '*'
                ni = sn + 1;
                star_ni = Some(ni);
                continue;
            }
        }

        // No match
        return false;
    }

    // Consumed both pattern and name
    true
}

/// Match a single byte, handling case sensitivity.
#[inline]
fn bytes_match_one(p: u8, n: u8, case_sensitive: bool) -> bool {
    if case_sensitive {
        p == n
    } else {
        p.eq_ignore_ascii_case(&n)
    }
}

/// Normalise a path for matching (on Windows, replace \\ with /).
fn normalise_for_match(path: &OsStr, is_windows: bool) -> Vec<u8> {
    if is_windows {
        path.as_encoded_bytes()
            .iter()
            .map(|&b| if b == b'\\' { b'/' } else { b })
            .collect()
    } else {
        path.as_encoded_bytes().to_vec()
    }
}

/// Match a path against a pattern, normalising Windows separators.
///
/// This is the top-level entry point used by PurePath.match().
pub fn match_path(pattern: &OsStr, path: &OsStr, case_sensitive: bool, is_windows: bool) -> bool {
    let path_normalised = normalise_for_match(path, is_windows);
    let compiled = GlobPattern::new(pattern, case_sensitive);
    compiled.matches(crate::from_os_bytes(&path_normalised))
}

/// Match a path against a pattern where the pattern must cover the entire path.
///
/// Unlike [`match_path`], a relative pattern like ``"*.py"`` will NOT match
/// ``"/a/b/foo.py"`` — the segment counts must match exactly.
///
/// This is the entry point used by PurePath.full_match().
pub fn full_match_path(
    pattern: &OsStr,
    path: &OsStr,
    case_sensitive: bool,
    is_windows: bool,
) -> bool {
    let path_normalised = normalise_for_match(path, is_windows);
    let pattern_normalised = normalise_for_match(pattern, is_windows);
    let compiled = GlobPattern::new(crate::from_os_bytes(&pattern_normalised), case_sensitive);
    compiled.full_matches(crate::from_os_bytes(&path_normalised))
}

/// Split a path byte slice into segments (including empty ones from leading slashes).
fn split_path_segments(path: &[u8]) -> Vec<&[u8]> {
    if path.is_empty() {
        return vec![&[]];
    }
    path.split(|&b| b == b'/').collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fnmatch_literal() {
        assert!(fnmatch_bytes(b"foo", b"foo", true));
        assert!(!fnmatch_bytes(b"foo", b"bar", true));
    }

    #[test]
    fn test_fnmatch_star() {
        assert!(fnmatch_bytes(b"*.txt", b"foo.txt", true));
        assert!(fnmatch_bytes(b"*", b"anything", true));
        assert!(fnmatch_bytes(b"f*o", b"foo", true));
        assert!(fnmatch_bytes(b"*.py", b"foo.py", true));
        assert!(!fnmatch_bytes(b"*.py", b"foo.txt", true));
    }

    #[test]
    fn test_fnmatch_question() {
        assert!(fnmatch_bytes(b"f?o", b"foo", true));
        assert!(fnmatch_bytes(b"?.txt", b"a.txt", true));
        assert!(!fnmatch_bytes(b"?.txt", b"ab.txt", true));
    }

    #[test]
    fn test_fnmatch_charclass() {
        assert!(fnmatch_bytes(b"[abc]", b"a", true));
        assert!(fnmatch_bytes(b"[abc]", b"b", true));
        assert!(!fnmatch_bytes(b"[abc]", b"d", true));
        assert!(fnmatch_bytes(b"[!abc]", b"d", true));
        assert!(!fnmatch_bytes(b"[!abc]", b"a", true));
    }

    #[test]
    fn test_fnmatch_complex() {
        assert!(fnmatch_bytes(b"*.[ch]", b"foo.c", true));
        assert!(fnmatch_bytes(b"*.[ch]", b"bar.h", true));
        assert!(!fnmatch_bytes(b"*.[ch]", b"baz.py", true));
        assert!(fnmatch_bytes(b"test_*.py", b"test_foo.py", true));
    }

    #[test]
    fn test_fnmatch_case_insensitive() {
        assert!(fnmatch_bytes(b"FOO", b"foo", false));
        assert!(fnmatch_bytes(b"*.TXT", b"readme.txt", false));
    }

    #[test]
    fn test_glob_pattern_absolute() {
        let p = GlobPattern::new(OsStr::new("/*.py"), true);
        assert!(p.matches(OsStr::new("/foo.py")));
        assert!(!p.matches(OsStr::new("/foo.txt")));
        assert!(!p.matches(OsStr::new("/sub/foo.py")));
    }

    #[test]
    fn test_glob_pattern_relative() {
        let p = GlobPattern::new(OsStr::new("*.py"), true);
        assert!(p.matches(OsStr::new("foo.py")));
        assert!(p.matches(OsStr::new("/bar/foo.py")));
        assert!(!p.matches(OsStr::new("foo.txt")));
    }

    #[test]
    fn test_match_path_windows() {
        assert!(match_path(
            OsStr::new("*.py"),
            OsStr::new("foo.py"),
            true,
            true,
        ));
        assert!(match_path(
            OsStr::new("*.py"),
            OsStr::new("C:\\bar\\foo.py"),
            true,
            true,
        ));
    }

    // -- full_match tests --

    #[test]
    fn test_full_match_relative() {
        // Full match: relative pattern, relative path (same segment count)
        assert!(full_match_path(
            OsStr::new("*.py"),
            OsStr::new("foo.py"),
            true,
            false,
        ));
        // Full match: relative pattern with multi-segment path should NOT match
        assert!(!full_match_path(
            OsStr::new("*.py"),
            OsStr::new("/a/b/foo.py"),
            true,
            false,
        ));
        // Full match: absolute pattern, absolute path
        assert!(full_match_path(
            OsStr::new("/foo/*.py"),
            OsStr::new("/foo/bar.py"),
            true,
            false,
        ));
        assert!(!full_match_path(
            OsStr::new("/foo/*.py"),
            OsStr::new("/foo/bar/baz.py"),
            true,
            false,
        ));
    }

    #[test]
    fn test_full_match_absolute() {
        assert!(full_match_path(
            OsStr::new("/a/b/c.py"),
            OsStr::new("/a/b/c.py"),
            true,
            false,
        ));
        assert!(!full_match_path(
            OsStr::new("/a/b"),
            OsStr::new("/a/b/c.py"),
            true,
            false,
        ));
    }

    #[test]
    fn test_full_match_case_insensitive() {
        assert!(full_match_path(
            OsStr::new("*.PY"),
            OsStr::new("foo.py"),
            false,
            false,
        ));
    }

    #[test]
    fn test_full_match_windows() {
        assert!(full_match_path(
            OsStr::new("*.py"),
            OsStr::new("foo.py"),
            true,
            true,
        ));
        // Full match: two-segment relative pattern vs Windows path (normalised)
        assert!(full_match_path(
            OsStr::new("bar/*.py"),
            OsStr::new("bar/foo.py"),
            true,
            true,
        ));
        // Full match: pattern that would match via tail (match) should not match here
        assert!(!full_match_path(
            OsStr::new("*.py"),
            OsStr::new("C:/bar/foo.py"),
            true,
            true,
        ));
    }
}
