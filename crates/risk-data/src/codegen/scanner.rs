//! Recursively walks a directory for `.toml` files and converts the result into a [`Dataset`] —
//! see [`Scanner`].
//!
//! Scanning is split out from parsing so the rest of [`super`] never touches the filesystem
//! directly: a [`Scanner`] only ever holds raw `(path, content)` pairs, sorted by path for a
//! deterministic merge order regardless of the filesystem's own directory-listing order.

use crate::{error::RiskDataError, types::Dataset};

use std::{fs, io, path::Path};

/// A `.toml` file's path (as discovered, suitable for an error message) paired with its content.
type TomlSource = (Box<str>, Box<str>);

/// Every `.toml` file found under a root directory, populated by [`Self::new`]. Always holds at
/// least one file: [`Self::collect_toml_files`] rejects an empty walk with
/// [`RiskDataError::NoTomlFiles`] before a `Scanner` is ever constructed.
#[derive(Debug, Clone)]
pub(super) struct Scanner(Vec<TomlSource>);

impl Scanner {
    /// Walks `dir` and collects every `.toml` file found under it, recursively.
    ///
    /// # Errors
    ///
    /// Returns [`RiskDataError::MissingRoot`] if `dir` doesn't exist or isn't a directory, or
    /// [`RiskDataError::NoTomlFiles`] if it is but contains no `.toml` file recursively.
    #[must_use = "constructing a scanner has no effect unless the caller reads its files"]
    pub(super) fn new<P>(dir: P) -> Result<Self, RiskDataError>
    where
        P: AsRef<Path>,
    {
        Ok(Self(Self::collect_toml_files(dir.as_ref())?))
    }

    /// Every `.toml` file under `dir`, recursively, as `(path, contents)` pairs sorted by path
    /// (for a deterministic merge order regardless of filesystem directory-listing order).
    ///
    /// # Errors
    ///
    /// Returns [`RiskDataError::MissingRoot`] if `dir` doesn't exist or isn't a directory, or
    /// [`RiskDataError::NoTomlFiles`] if it is but contains no `.toml` file recursively.
    fn collect_toml_files(root: &Path) -> Result<Vec<TomlSource>, RiskDataError> {
        if !root.is_dir() {
            return Err(RiskDataError::MissingRoot {
                path: root.to_string_lossy().into(),
            });
        }

        let mut files = Self::walk(root, Vec::default())?;

        if files.is_empty() {
            return Err(RiskDataError::NoTomlFiles {
                path: root.to_string_lossy().into(),
            });
        }

        files.sort_by(|(a, _), (b, _)| a.cmp(b));

        Ok(files)
    }

    /// `path`/`source` rendered as a [`RiskDataError::Io`].
    #[must_use = "building an error has no effect unless the caller returns it"]
    fn io_error(path: &Path, source: io::Error) -> RiskDataError {
        RiskDataError::Io {
            path: path.to_string_lossy().into_owned().into(),
            source: Box::new(source),
        }
    }

    /// Depth-first directory recursion backing [`Self::collect_toml_files`], threading `files`
    /// through the recursion and returning it with every `.toml` file found under `dir` appended.
    fn walk(dir: &Path, mut files: Vec<TomlSource>) -> Result<Vec<TomlSource>, RiskDataError> {
        let entries = fs::read_dir(dir).map_err(|source| Self::io_error(dir, source))?;

        for entry in entries {
            let entry = entry.map_err(|source| Self::io_error(dir, source))?;
            let path = entry.path();
            // `fs::metadata` (unlike `DirEntry::file_type`) follows symlinks, so a symlinked
            // subdirectory is walked instead of being silently skipped.
            let is_dir = fs::metadata(&path)
                .map_err(|source| Self::io_error(&path, source))?
                .is_dir();

            if is_dir {
                files = Self::walk(&path, files)?;
            } else if path.extension().is_some_and(|ext| ext == "toml") {
                let content =
                    fs::read_to_string(&path).map_err(|source| Self::io_error(&path, source))?;
                let name = path.to_string_lossy();

                files.push((name.into(), content.into()));
            } else {
                // Neither a directory to recurse into, nor a `.toml` file to read.
            }
        }

        Ok(files)
    }
}

impl TryFrom<Scanner> for Dataset {
    type Error = RiskDataError;

    /// Parses, merges, and validates every `.toml` file `scanner` found, consuming it — the
    /// caller never needs to know `Scanner` stores `(path, content)` pairs as `(Box<str>,
    /// Box<str>)`, only that a `Scanner` converts into a `Dataset`.
    ///
    /// # Errors
    ///
    /// Returns [`RiskDataError::Toml`] if any file's content isn't valid TOML or doesn't match
    /// the schema, or any error [`Dataset::validate`] reports for a cross-reference the schema
    /// alone can't catch.
    fn try_from(scanner: Scanner) -> Result<Self, Self::Error> {
        Self::parse(scanner.0)
    }
}
