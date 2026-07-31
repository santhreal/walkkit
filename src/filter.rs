//! File filtering using include and exclude glob patterns.

use globset::{Glob, GlobSet, GlobSetBuilder};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Defines rules for filtering files during a walk.
#[derive(Default, Clone)]
pub struct FileFilter {
    include_patterns: Vec<String>,
    exclude_patterns: Vec<String>,
}

impl FileFilter {
    /// Creates a new, empty file filter that accepts all files.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use walkkit::FileFilter;
    /// let filter = FileFilter::new();
    /// ```
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a glob pattern to the include list.
    ///
    /// If any include patterns are defined, a file must match at least one
    /// to be included in the results.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use walkkit::FileFilter;
    /// let filter = FileFilter::new().add_include("*.rs");
    /// ```
    #[must_use]
    pub fn add_include(mut self, pattern: &str) -> Self {
        self.include_patterns.push(pattern.to_string());
        self
    }

    /// Adds a glob pattern to the exclude list.
    ///
    /// If a file matches any exclude pattern, it is ignored, even if it
    /// matches an include pattern.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use walkkit::FileFilter;
    /// let filter = FileFilter::new().add_exclude("tests/**");
    /// ```
    #[must_use]
    pub fn add_exclude(mut self, pattern: &str) -> Self {
        self.exclude_patterns.push(pattern.to_string());
        self
    }

    /// Compiles the patterns into efficient matcher sets.
    ///
    /// # Errors
    /// Returns an error if any of the glob patterns are invalid.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use walkkit::FileFilter;
    /// let filter = FileFilter::new().add_include("*.rs").compile().unwrap();
    /// ```
    pub fn compile(&self) -> Result<CompiledFilter, crate::Error> {
        let mut include_builder = GlobSetBuilder::new();
        for pat in &self.include_patterns {
            if pat.is_empty() {
                return Err(crate::Error::InvalidFilterPattern {
                    message: "include pattern must not be empty".into(),
                });
            }
            if pat.contains('\0') {
                return Err(crate::Error::InvalidFilterPattern {
                    message: "include pattern must not contain NUL bytes".into(),
                });
            }
            include_builder.add(Glob::new(pat)?);
        }

        let mut exclude_builder = GlobSetBuilder::new();
        for pat in &self.exclude_patterns {
            if pat.is_empty() {
                return Err(crate::Error::InvalidFilterPattern {
                    message: "exclude pattern must not be empty".into(),
                });
            }
            if pat.contains('\0') {
                return Err(crate::Error::InvalidFilterPattern {
                    message: "exclude pattern must not contain NUL bytes".into(),
                });
            }
            exclude_builder.add(Glob::new(pat)?);
        }

        Ok(CompiledFilter {
            includes: include_builder.build()?,
            excludes: exclude_builder.build()?,
            has_includes: !self.include_patterns.is_empty(),
        })
    }
}

/// A compiled file filter ready for matching.
pub struct CompiledFilter {
    includes: GlobSet,
    excludes: GlobSet,
    has_includes: bool,
}

impl CompiledFilter {
    /// Checks whether the given path passes the filter.
    #[must_use]
    pub fn is_match(&self, path: &Path) -> bool {
        if self.excludes.is_match(path) {
            return false;
        }
        if self.has_includes && !self.includes.is_match(path) {
            return false;
        }
        true
    }
}

pub(crate) fn code_entry_allowed(
    path: &Path,
    root: &Path,
    config: &crate::codewalker::WalkConfig,
) -> crate::error::Result<bool> {
    let depth = symlink_depth(path)?;
    if !config.follow_symlinks {
        return Ok(depth == 0);
    }

    if depth > config.max_symlink_depth {
        return Ok(false);
    }

    if depth > 0 && has_symlink_loop(path)? {
        return Ok(false);
    }

    if depth > 0 {
        let canonical = std::fs::canonicalize(path)?;
        let root_canonical = std::fs::canonicalize(root)?;
        if !canonical.starts_with(&root_canonical) {
            return Ok(false);
        }
    }

    Ok(true)
}

pub(crate) fn code_process_path(
    path: &Path,
    config: &crate::codewalker::WalkConfig,
) -> crate::error::Result<Option<crate::codewalker::FileEntry>> {
    let mut file = std::fs::File::open(path)?;
    let metadata = file.metadata()?;
    let size = metadata.len();

    if config.max_file_size > 0 && size > config.max_file_size {
        return Ok(None);
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        let lower = ext.to_ascii_lowercase();
        if !config.include_extensions.is_empty() && !config.include_extensions.contains(&lower) {
            return Ok(None);
        }
        if config.exclude_extensions.contains(&lower) {
            return Ok(None);
        }
    } else if !config.include_extensions.is_empty() {
        return Ok(None);
    }

    let is_bin = if size == 0 {
        false
    } else {
        crate::detect::is_binary_file(path, &mut file)?
    };
    if config.skip_binary && is_bin {
        return Ok(None);
    }

    Ok(Some(crate::codewalker::FileEntry {
        path: path.to_path_buf(),
        size,
        is_binary: is_bin,
    }))
}

fn symlink_depth(path: &Path) -> std::io::Result<usize> {
    let mut depth = 0usize;
    let mut current = PathBuf::new();

    for component in path.components() {
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current)?;
        if metadata.file_type().is_symlink() {
            depth = depth.saturating_add(1);
            // Resolve this symlink so that symlinks nested inside the
            // resolved target are counted in the next iterations.
            if let Ok(resolved) = std::fs::canonicalize(&current) {
                current = resolved;
            }
        }
    }

    Ok(depth)
}

fn has_symlink_loop(path: &Path) -> std::io::Result<bool> {
    let mut seen = HashSet::new();
    let mut current = PathBuf::new();

    for component in path.components() {
        current.push(component);
        let metadata = std::fs::symlink_metadata(&current)?;
        if !metadata.file_type().is_symlink() {
            continue;
        }

        let Some(identity) = symlink_identity(&current, &metadata) else {
            continue;
        };
        if !seen.insert(identity) {
            return Ok(true);
        }
    }

    Ok(false)
}

#[cfg(unix)]
fn symlink_identity(_path: &Path, metadata: &std::fs::Metadata) -> Option<FileIdentity> {
    use std::os::unix::fs::MetadataExt;

    Some(FileIdentity {
        device: metadata.dev(),
        inode: metadata.ino(),
    })
}

#[cfg(windows)]
fn symlink_identity(_path: &Path, metadata: &std::fs::Metadata) -> Option<FileIdentity> {
    use std::os::windows::fs::MetadataExt;

    Some(FileIdentity {
        volume_serial: metadata.volume_serial_number()?.into(),
        file_index: metadata.file_index()?.into(),
    })
}

#[cfg(not(any(unix, windows)))]
fn symlink_identity(_: &Path, _: &std::fs::Metadata) -> Option<FileIdentity> {
    None
}

#[cfg(unix)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FileIdentity {
    device: u64,
    inode: u64,
}

#[cfg(windows)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FileIdentity {
    volume_serial: u64,
    file_index: u64,
}

#[cfg(not(any(unix, windows)))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct FileIdentity;
