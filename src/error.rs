//! Error types for code tree walking and content loading.

use std::io;

/// Errors that can occur during a code walk.
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum CodewalkError {
    /// An I/O error occurred while reading the file system.
    #[error(
        "I/O error: {0}. Fix: verify the path exists, is readable, and that the current process has permission to walk it."
    )]
    Io(#[from] io::Error),

    /// A file is too large to process.
    #[error(
        "file too large: {0} bytes. Fix: raise `max_file_size` or exclude the file before walking the tree."
    )]
    FileTooLarge(u64),

    /// An ignore rule or walk configuration error.
    #[error(
        "walk builder error: {0}. Fix: verify ignore rules, symlink handling, and walk configuration values."
    )]
    Ignore(#[from] ignore::Error),

    /// A UTF-8 decoding error occurred.
    #[error(
        "UTF-8 decoding error: {0}. Fix: call `content()` for raw bytes or skip non-UTF-8 files before decoding them as text."
    )]
    Utf8Error(#[from] std::string::FromUtf8Error),
}

/// A specialized Result type for codewalk.
pub type Result<T> = std::result::Result<T, CodewalkError>;
