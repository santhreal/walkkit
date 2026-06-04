//! Error handling types.
//!
//! Provides the exhaustive `ArchiveError` enum describing why reading or parsing failed.
//! Each error gives an actionable message.

use thiserror::Error;

/// Failure while walking tar or zip bytes.
#[derive(Debug, Error, Eq, PartialEq)]
#[non_exhaustive]
pub enum ArchiveError {
    /// Fewer bytes than required for the next header or file chunk.
    #[error(
        "truncated tar archive: need {need} bytes at offset {offset} (have {have}). Fix: pass the full archive slice."
    )]
    Truncated {
        /// Byte offset where truncation was detected.
        offset: usize,
        /// Bytes needed to continue parsing.
        need: usize,
        /// Bytes actually available in the input slice.
        have: usize,
    },
    /// Header block failed structural validation (checksum, octal fields, etc.).
    #[error(
        "invalid tar header at offset {offset}: {message}. Fix: verify the input is a valid ustar tar stream."
    )]
    InvalidHeader {
        /// Byte offset of the invalid header block.
        offset: usize,
        /// Explanation of why validation failed.
        message: String,
    },
    /// Declared size exceeds [`super::entry::MAX_ENTRY_BYTES`].
    #[error(
        "tar entry size {size} exceeds maximum ({max}). Fix: raise MAX_ENTRY_BYTES only if you accept that memory risk, or preprocess the archive."
    )]
    EntryTooLarge {
        /// Declared size from the tar header.
        size: u64,
        /// Maximum allowed entry size.
        max: usize,
    },
    /// Path contains '..' or starts with '/' (path traversal attempt).
    #[error(
        "tar entry name '{name}' contains path traversal pattern ('..' or absolute path). Fix: sanitize archive entries before processing."
    )]
    PathTraversal {
        /// The offending entry name.
        name: String,
    },
    /// Number of entries exceeds [`super::entry::MAX_ENTRIES`].
    #[error(
        "tar archive contains too many entries ({count} exceeds maximum {max}). Fix: raise MAX_ENTRIES only if you accept that resource risk, or split the archive."
    )]
    TooManyEntries {
        /// Actual entry count.
        count: usize,
        /// Maximum allowed entries.
        max: usize,
    },
}
