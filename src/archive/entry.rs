//! Extracted entry representations and security bounds.
//!
//! Defines what an extracted file looks like, as well as strict upper bounds
//! to prevent memory exhaustion from hostile archives.

/// Hard upper bound for a single entry payload to limit hostile archives (256 MB).
///
/// Adjust only with care: raising this increases peak memory use per [`ArchiveEntry::data`].
pub const MAX_ENTRY_BYTES: usize = 256 * 1024 * 1024;

/// Maximum number of entries to prevent hostile archives from exhausting resources.
pub const MAX_ENTRIES: usize = 1_000_000;

/// One regular-file payload extracted from an archive.
///
/// Only populated for regular files. Directories and links never appear.
/// On success, `data.len() == size`.
///
/// # Example
///
/// ```
/// use archivewalk::ArchiveEntry;
/// let entry = ArchiveEntry {
///     name: "config.json".to_string(),
///     size: 2,
///     data: vec![123, 125],
/// };
/// assert_eq!(entry.size as usize, entry.data.len());
/// ```
#[derive(Clone, Debug)]
pub struct ArchiveEntry {
    /// Path inside the archive (non-UTF-8 segments use Unicode replacement).
    pub name: String,
    /// Declared logical size in bytes (matches `data.len()` for successful reads).
    pub size: u64,
    /// Full file contents (length equals `size`). Ref-counted to avoid copies.
    pub data: std::sync::Arc<[u8]>,
}

impl PartialEq for ArchiveEntry {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name && self.size == other.size && *self.data == *other.data
    }
}

impl Eq for ArchiveEntry {}
