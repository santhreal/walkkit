//! Feature-gated archive walking and extraction support.

#![allow(
    clippy::cast_possible_truncation,
    clippy::redundant_closure_for_method_calls
)]

/// Streaming decompression helpers for gzip and zstd.
pub mod decompress;
/// Archive entry representation with metadata.
pub mod entry;
/// Error types for archive parsing operations.
pub mod error;
/// Recursive archive content extraction.
pub mod extract;
/// POSIX ustar tar reader and header parsing.
pub mod tar;
/// ZIP central-directory parser and entry iteration.
pub mod zip;

pub use crate::probe::format::{detect_format, DecompressFormat as ArchiveFormat};
pub use decompress::*;
pub use entry::*;
pub use error::*;
pub use extract::*;
pub use tar::*;
pub use zip::*;
