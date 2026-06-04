//! Core walker implementation and builder.

use crate::filter::{CompiledFilter, FileFilter};
use crate::iter::{WalkItemIter, WalkItemIterInner};
use crate::sort::SortMode;
use crate::walk_common::WalkOptions;
use crate::worker::{walk_multi_thread, walk_single_thread};
use crate::WalkItem;
use crossbeam_channel::{bounded, Receiver, Sender};
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;

/// Maximum byte length of any path we traverse or emit (bound TOCTOU / kernel limits).
///
/// Set to the Linux `PATH_MAX` (4096). Paths longer than this cannot be resolved
/// by the absolute-path syscalls the walker uses (the kernel returns
/// `ENAMETOOLONG`), so the walker rejects them early with its own
/// [`InvalidInput`](std::io::ErrorKind::InvalidInput) [`WalkItem::Error`](crate::WalkItem)
/// rather than deferring to a less actionable OS error.
pub const MAX_WALK_PATH_BYTES: usize = 4096;

pub(crate) fn path_exceeds_walk_limit(path: &Path) -> bool {
    path_byte_len(path) > MAX_WALK_PATH_BYTES
}

#[cfg(unix)]
fn path_byte_len(path: &Path) -> usize {
    use std::os::unix::ffi::OsStrExt;
    path.as_os_str().as_bytes().len()
}

#[cfg(not(unix))]
fn path_byte_len(path: &Path) -> usize {
    path.to_string_lossy().as_bytes().len()
}

/// A collision-free directory identifier for cycle detection.
///
/// On Unix this is `(dev, ino)`. On Windows this is `(volume_serial_number, file_index)`
/// from `GetFileInformationByHandle`. On other platforms, canonical path when following symlinks.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum DirId {
    #[cfg(unix)]
    Unix { device: u64, inode: u64 },
    #[cfg(windows)]
    Windows {
        volume_serial_number: u64,
        file_index: u64,
    },
    #[cfg(all(not(unix), not(windows)))]
    Other(PathBuf),
}

/// Stable id for a visited directory (used for symlink cycle detection when following links).
///
/// On Unix this is `(st_dev, st_ino)` from the same `stat`/`lstat` the walker already used, so it
/// matches the kernel’s notion of “this directory” even when reached via different pathnames.
pub(crate) fn dir_id(
    path: &Path,
    meta: &fs::Metadata,
    follow_symlinks: bool,
) -> std::io::Result<DirId> {
    dir_id_internal(path, meta, follow_symlinks)
}

#[cfg(unix)]
#[allow(clippy::unnecessary_wraps)] // Keeps the same `io::Result` signature as non-Unix `dir_id_internal`.
fn dir_id_internal(
    _path: &Path,
    meta: &fs::Metadata,
    _follow_symlinks: bool,
) -> std::io::Result<DirId> {
    use std::os::unix::fs::MetadataExt;
    Ok(DirId::Unix {
        device: meta.dev(),
        inode: meta.ino(),
    })
}

#[cfg(windows)]
fn dir_id_internal(
    _path: &Path,
    meta: &fs::Metadata,
    _follow_symlinks: bool,
) -> std::io::Result<DirId> {
    use std::os::windows::fs::MetadataExt;
    Ok(DirId::Windows {
        volume_serial_number: meta.volume_serial_number().unwrap_or(0),
        file_index: meta.file_index().unwrap_or(0),
    })
}

#[cfg(all(not(unix), not(windows)))]
fn dir_id_internal(
    path: &Path,
    _meta: &fs::Metadata,
    follow_symlinks: bool,
) -> std::io::Result<DirId> {
    if follow_symlinks {
        Ok(DirId::Other(path.canonicalize()?))
    } else {
        Ok(DirId::Other(path.to_path_buf()))
    }
}

/// A discovered file with its path and essential metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkedFile {
    /// Absolute or root-relative path to the file.
    pub path: PathBuf,
    /// File size in bytes from metadata at discovery time.
    pub size: u64,
    /// Platform inode (or equivalent) for deduplication / identity.
    pub inode: u64,
}

impl WalkedFile {
    /// Returns true if the file name starts with `.`.
    #[must_use]
    pub fn is_hidden(&self) -> bool {
        self.path
            .file_name()
            .map(std::ffi::OsStr::as_encoded_bytes)
            .is_some_and(|bytes| bytes.starts_with(b"."))
    }
}

/// Builder and configuration for parallel file discovery.
#[derive(Clone)]
pub struct Walker {
    roots: Vec<PathBuf>,
    filter: FileFilter,
    parallelism: usize,
    sort_mode: SortMode,
    follow_symlinks: bool,
    respect_gitignore: bool,
    skip_binary: bool,
    max_depth: Option<usize>,
    extension_filter: Option<String>,
    size_limit: Option<u64>,
    max_dir_entries: Option<usize>,
}

impl Default for Walker {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            filter: FileFilter::new(),
            parallelism: 4,
            sort_mode: SortMode::default(),
            follow_symlinks: false,
            respect_gitignore: false,
            skip_binary: false,
            max_depth: None,
            extension_filter: None,
            size_limit: None,
            max_dir_entries: Some(crate::walk_common::DEFAULT_MAX_DIR_ENTRIES),
        }
    }
}

impl Walker {
    /// Creates a walker with default parallelism and no roots (add roots before walking).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a directory root to traverse.
    #[must_use]
    pub fn add_root<P: AsRef<Path>>(mut self, root: P) -> Self {
        self.roots.push(root.as_ref().to_path_buf());
        self
    }

    /// Replaces include/exclude glob filters.
    #[must_use]
    pub fn with_filter(mut self, filter: FileFilter) -> Self {
        self.filter = filter;
        self
    }

    /// Sets worker thread count (clamped to a safe upper bound).
    #[must_use]
    pub fn with_parallelism(mut self, threads: usize) -> Self {
        let max_threads = std::thread::available_parallelism()
            .map(std::num::NonZeroUsize::get)
            .unwrap_or(1)
            .saturating_mul(4)
            .min(256);
        self.parallelism = threads.clamp(1, max_threads);
        self
    }

    /// Controls ordering of results when using [`Walker::walk`].
    #[must_use]
    pub fn with_sort(mut self, sort_mode: SortMode) -> Self {
        self.sort_mode = sort_mode;
        self
    }

    /// When true, follows symlinks to directories and files where the platform allows.
    #[must_use]
    pub fn follow_symlinks(mut self, follow: bool) -> Self {
        self.follow_symlinks = follow;
        self
    }

    /// When true and the `gitignore` feature is enabled, applies `.gitignore` rules.
    #[must_use]
    pub fn respect_gitignore(mut self, respect: bool) -> Self {
        self.respect_gitignore = respect;
        self
    }

    /// When true, skips files that appear to be binary (probe-based).
    #[must_use]
    pub fn skip_binary(mut self, skip: bool) -> Self {
        self.skip_binary = skip;
        self
    }

    /// Limits recursion depth from each root (inclusive).
    #[must_use]
    pub fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = Some(depth);
        self
    }

    /// Keeps only files whose extension matches (case-insensitive, leading dot optional).
    #[must_use]
    pub fn with_extension_filter(mut self, extension: &str) -> Self {
        let normalized = extension.trim_start_matches('.');
        self.extension_filter = Some(normalized.to_ascii_lowercase());
        self
    }

    /// Skips files larger than `max_bytes`.
    #[must_use]
    pub fn with_size_limit(mut self, max_bytes: u64) -> Self {
        self.size_limit = Some(max_bytes);
        self
    }

    /// Limits the number of entries read from a single directory.
    #[must_use]
    pub fn with_max_dir_entries(mut self, max: Option<usize>) -> Self {
        self.max_dir_entries = max;
        self
    }

    /// Walks all roots and yields [`WalkItem`] values (files and traversal errors).
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] if glob filters fail to compile.
    pub fn walk(self) -> std::result::Result<WalkItemIter, crate::Error> {
        let sort_mode = self.sort_mode;
        let rx = self.try_walk_parallel()?;

        if sort_mode == SortMode::Unsorted {
            Ok(WalkItemIter {
                inner: WalkItemIterInner::Streaming(rx),
            })
        } else {
            let items: Vec<WalkItem> = rx.into_iter().collect();
            let mut errors = Vec::new();
            let mut files = Vec::new();
            for it in items {
                match it {
                    WalkItem::Error(e) => errors.push(WalkItem::Error(e)),
                    WalkItem::File(f) => files.push(f),
                }
            }
            match sort_mode {
                SortMode::ByName => files.sort_by(|a, b| a.path.cmp(&b.path)),
                SortMode::BySize => files.sort_by_key(|f| f.size),
                SortMode::Unsorted => {}
            }
            let mut out = errors;
            out.extend(files.into_iter().map(WalkItem::File));
            Ok(WalkItemIter {
                inner: WalkItemIterInner::Buffered(out.into_iter()),
            })
        }
    }

    /// Starts the walk and returns a live channel of [`WalkItem`] until workers finish.
    ///
    /// # Errors
    ///
    /// Returns [`crate::Error`] if glob filters fail to compile.
    pub fn try_walk_parallel(self) -> std::result::Result<Receiver<WalkItem>, crate::Error> {
        let filter = self.filter.compile()?;
        let (tx, rx) = bounded(1024);
        self.spawn_walk_threads(filter, tx);
        Ok(rx)
    }

    fn spawn_walk_threads(self, filter: CompiledFilter, tx: Sender<WalkItem>) {
        let parallelism = self.parallelism;
        let roots = self.roots;
        let options = WalkOptions {
            follow_symlinks: self.follow_symlinks,
            respect_gitignore: self.respect_gitignore,
            skip_binary: self.skip_binary,
            max_depth: self.max_depth,
            extension_filter: self.extension_filter,
            size_limit: self.size_limit,
            max_dir_entries: self.max_dir_entries,
        };

        thread::spawn(move || {
            if parallelism <= 1 {
                walk_single_thread(roots, &filter, &options, &tx);
            } else {
                walk_multi_thread(roots, filter, &tx, parallelism, options);
            }
        });
    }
}
