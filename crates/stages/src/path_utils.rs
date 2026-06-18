//! Shared path-handling helpers used by multiple stages.

use std::path::Path;

/// Returns `file_path`'s final path component (its "basename"), using
/// [`Path`]'s semantics — `None` for an empty path, or one ending in `/`,
/// `.`, or `..`, where there's no well-defined "last named component".
pub fn basename(file_path: &str) -> Option<&str> {
    Path::new(file_path).file_name().and_then(|f| f.to_str())
}
