//! Shared file probing and enrichment primitives.

pub mod entropy;
pub mod file_type;
pub mod format;
pub mod pe;

pub use entropy::{entropy_bucket, shannon_entropy};
pub use file_type::{detect_file_type, FileType};
pub use format::{detect_format, DecompressFormat};
pub use pe::{parse_pe, PeMetadata};
