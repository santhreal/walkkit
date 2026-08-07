//! Shared walk helpers: metadata, binary sampling, and work-queue state.

use crate::filter::CompiledFilter;
use crate::walker::WalkedFile;
use crate::{WalkError, WalkOp};
use std::fs::{self, File};
use std::io::Read;
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
pub(crate) fn is_unresolvable_symlink(
    path: &Path,
    e: &std::io::Error,
    follow_symlinks: bool,
) -> bool {
    if !follow_symlinks {
        return false;
    }
    let is_dangling_or_loop = e.kind() == std::io::ErrorKind::NotFound || is_eloop(e);

    is_dangling_or_loop
        && fs::symlink_metadata(path)
            .map(|m| m.file_type().is_symlink())
            .unwrap_or(false)
}

#[cfg(unix)]
fn is_eloop(e: &std::io::Error) -> bool {
    e.raw_os_error() == Some(libc::ELOOP)
}

#[cfg(not(unix))]
fn is_eloop(_: &std::io::Error) -> bool {
    false
}

/// Detect a binary file by sampling its leading bytes for a NUL.
///
/// Binary formats (executables, images, archives, compiled objects) carry NUL
/// bytes within their header, so scanning a bounded prefix is sufficient to
/// classify them - and avoids reading whole multi-megabyte files just to decide
/// whether to skip one. We scan the first [`PREFIX_SCAN_BYTES`] (64 KiB) and
/// short-circuit on the first NUL. A file whose only NUL sits past that prefix
/// is treated as text, matching the prefix-sampling heuristic git/ripgrep use
/// (a real binary would have NULs far earlier). Previously this read the ENTIRE
/// file for anything up to 10 MiB, an O(file) pessimization on large text files.
fn scan_file_for_nul(file: &mut File) -> std::io::Result<bool> {
    const SAMPLE_SIZE: usize = 8192;
    const PREFIX_SCAN_BYTES: u64 = 64 * 1024;

    let mut buf = [0u8; SAMPLE_SIZE];
    let mut scanned: u64 = 0;
    while scanned < PREFIX_SCAN_BYTES {
        match file.read(&mut buf)? {
            0 => break,
            n => {
                if buf[..n].contains(&0) {
                    return Ok(true);
                }
                scanned += n as u64;
            }
        }
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
        scan_file_for_nul(&mut file)
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
        scan_file_for_nul(&mut file)
    }
}

/// Decide whether `path` is EXCLUDED by an extension filter of `required`.
///
/// `required == ""` means "keep only files that have no extension". The
/// comparison is done over the extension's raw OS bytes
/// ([`OsStr::as_encoded_bytes`]) with ASCII-case folding, NOT via a lossy
/// `to_str`. That matters for a filesystem walker that must handle non-UTF-8
/// names: the old `path.extension().and_then(OsStr::to_str)` mapped a non-UTF-8
/// extension to `None`, so such a file was mis-classified as "extensionless"
/// (wrongly KEPT when filtering for extensionless files, and never able to be
/// matched byte-for-byte). Comparing bytes fixes both: a non-UTF-8 extension
/// correctly counts as "has an extension" and matches `required` iff the bytes
/// are ASCII-case-equal.
fn extension_excluded(path: &Path, required: &str) -> bool {
    match (required.is_empty(), path.extension()) {
        // Want extensionless, file has none: keep.
        (true, None) => false,
        // Want extensionless but file has one, or want extension but file has none: exclude.
        (true, Some(_)) | (false, None) => true,
        // Want a specific extension: exclude unless the raw bytes match
        // ASCII-case-insensitively.
        (false, Some(ext)) => !ext
            .as_encoded_bytes()
            .eq_ignore_ascii_case(required.as_bytes()),
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
    if extension_filter.is_some_and(|required| extension_excluded(&path, required)) {
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
                                "Fix: directory contains more than {max} entries; increase max_dir_entries or split the directory."
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
    /// Queue items: (path, depth, `walk_root`, pre-fetched metadata).
    ///
    /// The metadata is `Some` when the parent worker already stat-ed this entry
    /// while scanning its directory children (see `worker.rs` child scan), so the
    /// popping worker reuses it instead of issuing a second, redundant `stat` for
    /// the same path. Roots are enqueued with `None` (never stat-ed yet).
    pub(crate) queue: Vec<(PathBuf, usize, PathBuf, Option<std::fs::Metadata>)>,
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

#[cfg(test)]
mod extension_filter_tests {
    use super::extension_excluded;
    use std::path::Path;

    #[test]
    fn ascii_extension_matching_is_case_insensitive() {
        assert!(!extension_excluded(Path::new("a.rs"), "rs"));
        assert!(!extension_excluded(Path::new("a.RS"), "rs"), "ASCII case-insensitive");
        assert!(!extension_excluded(Path::new("a.rs"), "RS"));
        assert!(extension_excluded(Path::new("a.rs"), "py"), "non-matching ext excluded");
        assert!(extension_excluded(Path::new("archive.tar.gz"), "tar"), "only final ext counts");
        assert!(!extension_excluded(Path::new("archive.tar.gz"), "gz"));
    }

    #[test]
    fn empty_required_keeps_only_extensionless() {
        assert!(!extension_excluded(Path::new("Makefile"), ""), "no ext, want extensionless: keep");
        assert!(!extension_excluded(Path::new(".bashrc"), ""), "dotfile has no extension: keep");
        assert!(extension_excluded(Path::new("a.rs"), ""), "has ext, want extensionless: exclude");
    }

    #[test]
    fn missing_extension_excluded_when_specific_required() {
        assert!(extension_excluded(Path::new("Makefile"), "rs"), "no ext, want rs: exclude");
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_extension_is_compared_by_bytes_not_dropped() {
        use std::ffi::{OsStr, OsString};
        use std::os::unix::ffi::OsStrExt;
        use std::path::PathBuf;

        // "file." + the non-UTF-8 bytes 0xFF 0xFE as the extension.
        let mut name = OsString::from("file.");
        name.push(OsStr::from_bytes(&[0xFF, 0xFE]));
        let path = PathBuf::from(name);

        // Regression: the old `path.extension().and_then(OsStr::to_str)` mapped
        // this to None, so the file was mis-read as EXTENSIONLESS and wrongly
        // KEPT when filtering for extensionless files. It has an extension, so
        // it must be EXCLUDED.
        assert!(
            extension_excluded(&path, ""),
            "a non-UTF-8 extension must count as HAVING an extension (excluded from an extensionless filter)"
        );
        // And its non-UTF-8 bytes never equal an ASCII required extension.
        assert!(
            extension_excluded(&path, "rs"),
            "non-UTF-8 extension does not match the ASCII 'rs' filter"
        );

        // A valid ASCII extension on an otherwise non-UTF-8 filename still
        // matches (byte compare), proving no regression for that case.
        let mut stem = OsString::from("da");
        stem.push(OsStr::from_bytes(&[0xFF]));
        stem.push(OsStr::from_bytes(b".rs"));
        let mixed = PathBuf::from(stem);
        assert!(
            !extension_excluded(&mixed, "rs"),
            "a .rs file with a non-UTF-8 stem must still match the rs filter"
        );
    }
}

#[cfg(test)]
mod nul_scan_tests {
    use super::scan_file_for_nul;
    use std::io::{Seek, SeekFrom, Write};

    fn scan(bytes: &[u8]) -> bool {
        let mut f = tempfile::tempfile().expect("temp file");
        f.write_all(bytes).unwrap();
        f.seek(SeekFrom::Start(0)).unwrap();
        scan_file_for_nul(&mut f).unwrap()
    }

    #[test]
    fn nul_in_prefix_is_binary() {
        let mut v = b"text before".to_vec();
        v.push(0);
        v.extend_from_slice(b"more");
        assert!(scan(&v), "a NUL in the leading bytes marks the file binary");
    }

    #[test]
    fn pure_text_is_not_binary() {
        // 100 KiB of text (larger than the 64 KiB prefix) with no NUL.
        let v = vec![b'a'; 100 * 1024];
        assert!(!scan(&v), "NUL-free text must not be classified binary");
    }

    #[test]
    fn nul_past_prefix_is_treated_as_text() {
        // Regression for walk_common.rs:36: only the first 64 KiB is sampled, so
        // a lone NUL past the prefix (not a real binary) is treated as text and
        // the whole multi-megabyte file is NOT read. 64 KiB of text, then a NUL.
        let mut v = vec![b'a'; 64 * 1024];
        v.push(0);
        v.extend_from_slice(&vec![b'b'; 1024]);
        assert!(!scan(&v), "NUL past the sampled prefix is not detected (bounded scan)");
    }

    #[test]
    fn nul_at_edge_of_prefix_is_binary() {
        // A NUL within the 64 KiB window is still caught.
        let mut v = vec![b'a'; 60 * 1024];
        v.push(0);
        assert!(scan(&v), "NUL inside the prefix window is detected");
    }
}
