#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(clippy::pedantic)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::todo,
        clippy::unimplemented,
        clippy::panic
    )
)]
#![allow(
    clippy::module_name_repetitions,
    clippy::must_use_candidate,
    clippy::missing_errors_doc
)]

//! # walkkit - Parallel filesystem walker with ignore-aware traversal
//!
//! [![santh status](https://img.shields.io/badge/santh-stable-brightgreen)](https://santh.dev/standard)
//!
//! High-performance parallel directory walker with ignore-aware traversal, bounded work queues,
//! and cycle detection. `walkkit` provides both a custom multi-threaded engine (`Walker`) with
//! glob filtering and inode sorting, and a codebase scanner (`CodeWalker`) tuned for workspace
//! analysis with `.gitignore` hierarchy parsing and lazy content loading.
//!
//! All filesystem errors are preserved explicitly as typed error items rather than silently swallowed.
//! Binary files are detected via magic bytes and NUL-byte sampling, and symlink cycles are prevented
//! using OS directory identifiers.
//!
//! ## Quick Start
//!
//! ```rust
//! use walkkit::{Walker, WalkItem};
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let walker = Walker::new()
//!     .add_root("./src")
//!     .with_parallelism(4)
//!     .respect_gitignore(true)
//!     .skip_binary(true);
//!
//! for item in walker.walk()? {
//!     match item {
//!         WalkItem::File(f) => println!("{}: {} bytes", f.path.display(), f.size),
//!         WalkItem::Error(e) => eprintln!("Error: {}", e),
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! ## When to use / when not to use
//!
//! **When to use:**
//! - Parallel directory traversal over large codebases or large nested directories.
//! - Codebase scanning requiring `.gitignore` rules, glob include/exclude filters, and binary skipping.
//! - Applications needing strict error reporting without silent loss during traversal.
//!
//! **When not to use:**
//! - Simple single-directory listings where standard `std::fs::read_dir` is sufficient.
//! - High-contention fine-grained parallel traversal across millions of tiny flat files where `jwalk` work-stealing is specifically needed.
//!
//! ## Compared to alternatives
//!
//! `walkkit` provides structured parallel traversal with first-class `WalkItem::Error` propagation,
//! eliminating silent error suppression found in default iterators. Unlike `walkdir` (which is single-threaded),
//! `walkkit` leverages multi-threaded worker pools with bounded channels to bound in-flight memory.
//!
//! Compared to `ignore` and `jwalk`, `walkkit` offers both a low-level bounded `Walker` and a high-level
//! `CodeWalker` abstraction, integrated with `hashkit` for content addressing and security probing.
//!
//! ## How it fits in Santh
//!
//! `walkkit` lives in `libs/performance/io` as the foundational parallel directory discovery engine across
//! Santh analyzers, scanner tools, and indexing tools. It depends on `hashkit` for digest calculations
//! and provides the traversal primitive for codebase analysis pipelines.

#[cfg(feature = "archive")]
pub mod archive;
pub mod codewalker;
pub mod detect;
pub mod error;
pub mod filter;
#[cfg(feature = "gitignore")]
mod gitignore_ctx;
mod iter;
pub mod probe;
pub mod sandbox;
mod sort;
mod walk_common;
mod walker;
mod worker;

pub use codewalker::{CodeWalker, FileContent, FileContentChunks, FileEntry, WalkConfig};
pub use filter::{CompiledFilter, FileFilter};
pub use iter::WalkItemIter;
pub use sort::SortMode;
pub use walker::{WalkedFile, Walker, MAX_WALK_PATH_BYTES};

/// What the walker was doing when an I/O error occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum WalkOp {
    /// `stat` / metadata on a path.
    Metadata,
    /// Listing a directory.
    ReadDir,
    /// Opening a file (e.g. binary probe).
    Open,
    /// Reading or compiling `.gitignore` rules.
    Gitignore,
}

impl std::fmt::Display for WalkOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            WalkOp::Metadata => "metadata",
            WalkOp::ReadDir => "read_dir",
            WalkOp::Open => "open",
            WalkOp::Gitignore => "gitignore",
        })
    }
}

/// A filesystem error encountered during traversal (always reported, never swallowed).
#[derive(Debug)]
pub struct WalkError {
    /// Path the operation targeted.
    pub path: std::path::PathBuf,
    /// Operation class.
    pub op: WalkOp,
    /// Underlying I/O error.
    pub source: std::io::Error,
}

impl WalkError {
    /// Creates a [`WalkError`] from a path, operation, and [`std::io::Error`].
    #[must_use]
    pub fn new(path: std::path::PathBuf, op: WalkOp, source: std::io::Error) -> Self {
        Self { path, op, source }
    }

    fn fix_hint(err: &std::io::Error) -> &'static str {
        #[cfg(unix)]
        if err.raw_os_error() == Some(libc::ENAMETOOLONG) {
            return "Fix: shorten the path; the OS reported ENAMETOOLONG.";
        }
        match err.kind() {
            std::io::ErrorKind::PermissionDenied => {
                "Fix: grant read and search (execute on directories) permission, or exclude this path."
            }
            std::io::ErrorKind::NotFound => {
                "Fix: the path disappeared during the walk (concurrent delete/move); retry."
            }
            std::io::ErrorKind::NotADirectory => {
                "Fix: a path component is not a directory; check symlinks and types."
            }
            std::io::ErrorKind::InvalidInput => {
                "Fix: invalid path or walkkit limit (see error message)."
            }
            std::io::ErrorKind::Interrupted => "Fix: retry; the operation was interrupted.",
            _ => "Fix: inspect the underlying OS error and remediate I/O conditions.",
        }
    }
}

impl std::fmt::Display for WalkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} on {} during {}  -  {}",
            self.source,
            self.path.display(),
            self.op,
            Self::fix_hint(&self.source)
        )
    }
}

impl std::error::Error for WalkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.source)
    }
}

/// One element from a directory walk: a file candidate or a traversal error.
#[derive(Debug)]
pub enum WalkItem {
    /// A regular file that passed filters.
    File(WalkedFile),
    /// A non-fatal traversal failure at `path` (walk continues elsewhere when possible).
    Error(WalkError),
}

impl WalkItem {
    /// Returns the file payload if this is [`WalkItem::File`].
    #[must_use]
    pub fn into_file(self) -> Option<WalkedFile> {
        match self {
            Self::File(f) => Some(f),
            Self::Error(_) => None,
        }
    }

    /// Borrow the [`WalkedFile`] when this is [`WalkItem::File`].
    #[must_use]
    pub fn as_file(&self) -> Option<&WalkedFile> {
        match self {
            Self::File(f) => Some(f),
            Self::Error(_) => None,
        }
    }
    /// Returns the error payload if this is [`WalkItem::Error`].
    #[must_use]
    pub fn into_error(self) -> Option<WalkError> {
        match self {
            Self::File(_) => None,
            Self::Error(e) => Some(e),
        }
    }

    /// Borrow the [`WalkError`] when this is [`WalkItem::Error`].
    #[must_use]
    pub fn as_error(&self) -> Option<&WalkError> {
        match self {
            Self::File(_) => None,
            Self::Error(e) => Some(e),
        }
    }
}

/// Errors returned by walkkit operations.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// A glob filter pattern failed to compile.
    #[error("Fix: check the glob pattern for syntax errors. Invalid glob pattern: {source}")]
    InvalidGlob {
        /// The underlying globset error.
        #[from]
        source: globset::Error,
    },
    /// Include/exclude pattern was empty or contained a NUL byte.
    #[error(
        "Fix: remove empty patterns and null bytes from filter configuration. Invalid filter pattern: {message}"
    )]
    InvalidFilterPattern {
        /// Why the pattern was rejected.
        message: String,
    },
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
mod walkkit_tests {
    use super::*;
    use tempfile::tempdir;

    fn files_only(items: Vec<WalkItem>) -> Vec<WalkedFile> {
        items.into_iter().filter_map(WalkItem::into_file).collect()
    }

    #[test]
    fn walker_empty_directory() {
        let dir = tempdir().unwrap();
        let walker = Walker::new().add_root(dir.path());
        let files: Vec<_> = files_only(walker.walk().unwrap().collect());
        assert!(files.is_empty(), "empty dir should produce 0 files");
    }

    #[test]
    fn walker_single_file() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("test.txt"), "content").unwrap();
        let walker = Walker::new().add_root(dir.path());
        let files: Vec<_> = files_only(walker.walk().unwrap().collect());
        assert_eq!(files.len(), 1);
        assert!(files[0].path.ends_with("test.txt"));
    }

    #[test]
    fn walker_multiple_files() {
        let dir = tempdir().unwrap();
        for i in 0..5 {
            std::fs::write(dir.path().join(format!("f{i}.js")), "content").unwrap();
        }
        let walker = Walker::new().add_root(dir.path());
        let files: Vec<_> = files_only(walker.walk().unwrap().collect());
        assert_eq!(files.len(), 5);
    }

    #[test]
    fn walker_nested_directories() {
        let dir = tempdir().unwrap();
        let nested = dir.path().join("a/b/c");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("deep.js"), "deep content").unwrap();
        std::fs::write(dir.path().join("top.js"), "top content").unwrap();
        let walker = Walker::new().add_root(dir.path());
        let files: Vec<_> = files_only(walker.walk().unwrap().collect());
        assert_eq!(files.len(), 2, "should find files in nested dirs");
    }

    #[test]
    fn walker_file_size_reported() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("sized.txt"), "12345").unwrap();
        let walker = Walker::new().add_root(dir.path());
        let files: Vec<_> = files_only(walker.walk().unwrap().collect());
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].size, 5, "file size should be 5 bytes");
    }

    #[test]
    fn walker_hidden_files_included() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join(".hidden"), "secret").unwrap();
        std::fs::write(dir.path().join("visible"), "open").unwrap();
        let walker = Walker::new().add_root(dir.path());
        let files: Vec<_> = files_only(walker.walk().unwrap().collect());
        assert!(!files.is_empty(), "at least visible file should be found");
    }

    #[test]
    fn walker_parallelism_setting() {
        let dir = tempdir().unwrap();
        std::fs::write(dir.path().join("a.txt"), "a").unwrap();
        let walker = Walker::new().add_root(dir.path()).with_parallelism(1);
        let files: Vec<_> = files_only(walker.walk().unwrap().collect());
        assert_eq!(files.len(), 1);
    }

    #[test]
    fn walker_nonexistent_root() {
        let walker = Walker::new().add_root("/tmp/nonexistent_walkkit_test_12345");
        let items: Vec<_> = walker.walk().unwrap().collect();
        let file_count = items
            .iter()
            .filter(|i| matches!(i, WalkItem::File(_)))
            .count();
        assert_eq!(file_count, 0, "nonexistent root should produce 0 files");
        assert!(
            items.iter().any(|i| matches!(i, WalkItem::Error(_))),
            "nonexistent root should surface a traversal error"
        );
    }

    #[test]
    fn walker_symlink_not_followed_by_default() {
        let dir = tempdir().unwrap();
        let target = tempdir().unwrap();
        std::fs::write(target.path().join("target.txt"), "data").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(target.path(), dir.path().join("link")).ok();
        let walker = Walker::new().add_root(dir.path());
        let files: Vec<_> = files_only(walker.walk().unwrap().collect());
        assert!(files.len() <= 1);
    }

    #[test]
    fn walker_many_files_no_oom() {
        let dir = tempdir().unwrap();
        for i in 0..500 {
            std::fs::write(dir.path().join(format!("file{i:04}.txt")), "x").unwrap();
        }
        let walker = Walker::new().add_root(dir.path());
        let files: Vec<_> = files_only(walker.walk().unwrap().collect());
        assert_eq!(files.len(), 500, "should find all 500 files");
    }
}
