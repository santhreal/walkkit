//! Shared walk helpers: metadata, binary sampling, and work-queue state.

use crate::filter::CompiledFilter;
use crate::walker::WalkedFile;
use crate::{WalkError, WalkOp};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub(crate) fn metadata_for_path(
    path: &Path,
    follow_symlinks: bool,
) -> std::io::Result<fs::Metadata> {
    if follow_symlinks {
        fs::metadata(path)
    } else {
        fs::symlink_metadata(path)
    }
}

/// True when [`metadata_for_path`] failed because `path` is a symlink we cannot
/// resolve while following links -- a loop (ELOOP/`FilesystemLoop`) or a dangling
/// target. Such entries are non-traversable rather than hard errors, so callers
/// skip them silently instead of emitting a `Metadata` error per broken link.
pub(crate) fn is_unresolvable_symlink(path: &Path, follow_symlinks: bool) -> bool {
    follow_symlinks
        && fs::symlink_metadata(path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
}

fn scan_file_for_nul(file: &mut File, file_len: u64) -> std::io::Result<bool> {
    const SAMPLE_SIZE: usize = 8192;
    const FULL_READ_LIMIT: u64 = 10 * 1024 * 1024;

    if file_len <= FULL_READ_LIMIT {
        let mut buf = [0u8; SAMPLE_SIZE];
        loop {
            match file.read(&mut buf) {
                Ok(0) => return Ok(false),
                Ok(n) => {
                    if buf[..n].contains(&0) {
                        return Ok(true);
                    }
                }
                Err(e) => return Err(e),
            }
        }
    }

    let mut buf = [0u8; SAMPLE_SIZE];
    let stride = 64 * 1024u64;
    let mut offset = 0u64;

    while offset < file_len {
        file.seek(SeekFrom::Start(offset))?;
        let n = file.read(&mut buf)?;
        if buf[..n].contains(&0) {
            return Ok(true);
        }
        offset += stride;
    }

    Ok(false)
}

fn is_binary_same_fd(
    path: &Path,
    meta: &fs::Metadata,
    follow_symlinks: bool,
) -> std::io::Result<bool> {
    #[cfg(unix)]
    {
        use std::fs::OpenOptions;
        use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
        let mut file = if follow_symlinks {
            OpenOptions::new().read(true).open(path)?
        } else {
            OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(path)?
        };
        let got = file.metadata()?;
        if got.ino() != meta.ino() || got.dev() != meta.dev() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Fix: file identity changed between stat and open (TOCTOU); retry the walk.",
            ));
        }
        scan_file_for_nul(&mut file, got.len())
    }
    #[cfg(not(unix))]
    {
        let mut file = File::open(path)?;
        let got = file.metadata()?;
        if got.len() != meta.len() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Fix: file size changed between stat and open (TOCTOU); retry the walk.",
            ));
        }
        scan_file_for_nul(&mut file, got.len())
    }
}

pub(crate) fn build_walked_file(
    path: PathBuf,
    meta: &fs::Metadata,
    filter: &CompiledFilter,
    extension_filter: Option<&str>,
    size_limit: Option<u64>,
    skip_binary: bool,
    follow_symlinks: bool,
) -> Result<Option<WalkedFile>, WalkError> {
    if !meta.is_file() || !filter.is_match(&path) {
        return Ok(None);
    }
    if size_limit.is_some_and(|max_bytes| meta.len() > max_bytes) {
        return Ok(None);
    }
    if extension_filter.is_some_and(|required| {
        let actual_ext = path.extension().and_then(std::ffi::OsStr::to_str);
        match (required.is_empty(), actual_ext) {
            (true, None) => false,
            (true, Some(_)) | (false, None) => true,
            (false, Some(ext)) => !ext.eq_ignore_ascii_case(required),
        }
    }) {
        return Ok(None);
    }
    if skip_binary {
        let is_bin = is_binary_same_fd(&path, meta, follow_symlinks)
            .map_err(|e| WalkError::new(path.clone(), WalkOp::Open, e))?;
        if is_bin {
            return Ok(None);
        }
    }

    let inode = {
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            meta.ino()
        }
        #[cfg(not(unix))]
        {
            0
        }
    };
    Ok(Some(WalkedFile {
        path,
        size: meta.len(),
        inode,
    }))
}

/// Result of reading a directory: sorted child paths and any `read_dir` iterator errors.
///
/// Individual `DirEntry` failures are surfaced here instead of being dropped, so permission
/// races or transient I/O errors cannot hide filenames from the walk.
pub(crate) struct ReadDirSorted {
    /// Successfully enumerated child paths (sorted by [`OsStr`](std::ffi::OsStr) bytes).
    pub(crate) paths: Vec<PathBuf>,
    /// Errors from the underlying [`ReadDir`](std::fs::ReadDir) iterator (path is the parent dir).
    pub(crate) entry_errors: Vec<std::io::Error>,
}

/// Default maximum number of entries to read from a single directory.
pub const DEFAULT_MAX_DIR_ENTRIES: usize = 1_000_000;

pub(crate) fn read_dir_sorted(
    path: &Path,
    max_entries: Option<usize>,
) -> std::io::Result<ReadDirSorted> {
    let read_dir = fs::read_dir(path)?;
    let mut paths = Vec::new();
    let mut entry_errors = Vec::new();
    for ent in read_dir {
        match ent {
            Ok(e) => {
                paths.push(e.path());
                if let Some(max) = max_entries {
                    if paths.len() > max {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!(
                                "Fix: directory contains more than {} entries; increase max_dir_entries or split the directory.",
                                max
                            ),
                        ));
                    }
                }
            }
            Err(e) => entry_errors.push(e),
        }
    }
    paths.sort_unstable_by(|a, b| a.as_os_str().cmp(b.as_os_str()));
    Ok(ReadDirSorted {
        paths,
        entry_errors,
    })
}

pub(crate) struct WorkState {
    pub(crate) active: usize,
    pub(crate) queue: Vec<(PathBuf, usize, PathBuf)>,
    pub(crate) visited_dirs: std::collections::HashSet<crate::walker::DirId>,
    pub(crate) visited_files: std::collections::HashSet<PathBuf>,
}

#[derive(Clone)]
pub(crate) struct WalkOptions {
    pub(crate) follow_symlinks: bool,
    pub(crate) respect_gitignore: bool,
    pub(crate) skip_binary: bool,
    pub(crate) max_depth: Option<usize>,
    pub(crate) extension_filter: Option<String>,
    pub(crate) size_limit: Option<u64>,
    pub(crate) max_dir_entries: Option<usize>,
}

pub(crate) fn lock_work_state(
    mutex: &std::sync::Mutex<WorkState>,
) -> std::sync::MutexGuard<'_, WorkState> {
    match mutex.lock() {
        Ok(guard) => guard,
        Err(error) => error.into_inner(),
    }
}

pub(crate) fn wait_for_work<'a>(
    condvar: &std::sync::Condvar,
    guard: std::sync::MutexGuard<'a, WorkState>,
) -> std::sync::MutexGuard<'a, WorkState> {
    match condvar.wait(guard) {
        Ok(guard) => guard,
        Err(error) => error.into_inner(),
    }
}
