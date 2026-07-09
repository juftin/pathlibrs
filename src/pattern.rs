//! Glob pattern matching (fnmatch-style) for path ``.match()``.
//!
//! Implements the subset of fnmatch used by CPython's pathlib:
//! ``*``, ``?``, ``[seq]``, ``[!seq]``.

use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;

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
        let bytes = pattern.as_bytes();
        let is_absolute = bytes.first().map_or(false, |&b| b == b'/');

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
    pub fn matches(&self, path: &OsStr) -> bool {
        let path_bytes = path.as_bytes();

        // If pattern is relative, match against the tail of the path
        if !self.is_absolute {
            // Try matching at every position (right-anchored)
            return self.match_relative(path_bytes);
        }

        // Absolute pattern — must match from the start (after any root/drive)
        // For simplicity, just split the path and match segment-by-segment
        self.match_absolute(path_bytes)
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
    fn match_segments(&self, path_segments: &[&[u8]]) -> bool {
        if self.segments.len() != path_segments.len() {
            return false;
        }

        // Special case: empty pattern matches empty path
        if self.segments.is_empty() {
            return path_segments.is_empty();
        }

        // All segments except the last must match exactly
        for i in 0..self.segments.len().saturating_sub(1) {
            if !bytes_equal(&self.segments[i], path_segments[i], self.case_sensitive) {
                return false;
            }
        }

        // Last segment uses fnmatch
        if let (Some(pat), Some(path_seg)) = (self.segments.last(), path_segments.last()) {
            fnmatch_bytes(pat, path_seg, self.case_sensitive)
        } else {
            false
        }
    }
}

/// Compare two byte slices, optionally case-insensitive (ASCII only).
fn bytes_equal(a: &[u8], b: &[u8], case_sensitive: bool) -> bool {
    if a.len() != b.len() {
        return false;
    }
    if case_sensitive {
        a == b
    } else {
        a.iter()
            .zip(b.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
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
                b'[' => {
                    // Character class: [abc] or [!abc]
                    if ni < name.len() {
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
                }
                _ => {}
            }
        }

        // Literal match fallback or star backtracking
        if pi < pattern.len() && ni < name.len() {
            if bytes_match_one(pattern[pi], name[ni], case_sensitive) {
                pi += 1;
                ni += 1;
                continue;
            }
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

/// Match a path against a pattern, normalising Windows separators.
///
/// This is the top-level entry point used by PurePath.match().
pub fn match_path(pattern: &OsStr, path: &OsStr, case_sensitive: bool, is_windows: bool) -> bool {
    // On Windows, normalise backslashes to forward slashes in the path
    // before matching, so patterns written with / work against both.
    let path_normalised: Vec<u8> = if is_windows {
        path.as_bytes()
            .iter()
            .map(|&b| if b == b'\\' { b'/' } else { b })
            .collect()
    } else {
        path.as_bytes().to_vec()
    };

    let compiled = GlobPattern::new(pattern, case_sensitive);
    compiled.matches(OsStr::from_bytes(&path_normalised))
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
}
