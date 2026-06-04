//! File type classification from magic bytes and script/source heuristics.

/// Binary magic prefixes recognized by [`crate::detect::is_binary`].
pub(crate) const BINARY_MAGIC_BYTES: &[&[u8]] = &[
    b"\x7fELF",
    b"MZ",
    b"\xfe\xed\xfa\xce",
    b"\xfe\xed\xfa\xcf",
    b"\xce\xfa\xed\xfe",
    b"\xcf\xfa\xed\xfe",
    b"\xca\xfe\xba\xbe",
    b"\x00asm",
    b"PK\x03\x04",
    b"\x1f\x8b",
    b"BZ",
    b"\xfd7zXZ",
    b"\x89PNG",
    b"\xff\xd8\xff",
    b"GIF8",
    b"RIFF",
    b"\x00\x00\x01\x00",
    b"SQLite format 3",
    b"\x04\x22\x4d\x18",
    b"\x28\xb5\x2f\xfd",
];

/// High-level file classification used by enrichment consumers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FileType {
    /// File type could not be determined.
    Unknown,
    /// JavaScript source or bundle.
    JavaScript,
    /// Python source or script.
    Python,
    /// Rust source file.
    Rust,
    /// Windows Portable Executable.
    PE,
    /// ELF binary.
    ELF,
    /// Mach-O binary.
    MachO,
    /// Archive format.
    Archive,
    /// Image format.
    Image,
}

/// Detect the file type from raw bytes.
#[must_use]
pub fn detect_file_type(bytes: &[u8]) -> FileType {
    if bytes.len() >= 2 && &bytes[..2] == b"MZ" {
        return FileType::PE;
    }

    if bytes.len() >= 4 && &bytes[..4] == b"\x7fELF" {
        return FileType::ELF;
    }

    if bytes.len() >= 4 {
        let magic = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if matches!(
            magic,
            0xcafebabe | 0xfeedface | 0xfeedfacf | 0xcefaedfe | 0xcffaedfe
        ) {
            return FileType::MachO;
        }
    }

    if bytes.len() >= 4
        && (&bytes[..4] == b"PK\x03\x04"
            || &bytes[..4] == b"PK\x05\x06"
            || &bytes[..4] == b"\x28\xb5\x2f\xfd"
            || &bytes[..4] == b"\x04\x22\x4d\x18")
    {
        return FileType::Archive;
    }

    if bytes.len() >= 2 && (&bytes[..2] == b"\x1f\x8b" || &bytes[..2] == b"BZ") {
        return FileType::Archive;
    }

    if bytes.len() >= 4
        && (&bytes[..4] == b"\x89PNG" || &bytes[..4] == b"GIF8" || &bytes[..2] == b"BM")
    {
        return FileType::Image;
    }

    if bytes.len() >= 3 && &bytes[..3] == b"\xff\xd8\xff" {
        return FileType::Image;
    }

    let Ok(text) = std::str::from_utf8(bytes) else {
        return FileType::Unknown;
    };
    let trimmed = text.trim_start();

    if trimmed.starts_with("#!/usr/bin/env python")
        || trimmed.starts_with("#!/usr/bin/python")
        || trimmed.starts_with("#! /usr/bin/env python")
        || trimmed.starts_with("#! /usr/bin/python")
        || trimmed.starts_with("import ")
        || trimmed.starts_with("from ")
        || trimmed.starts_with("def ")
        || trimmed.starts_with("class ")
    {
        return FileType::Python;
    }

    if trimmed.starts_with("#!/usr/bin/env node")
        || trimmed.starts_with("#!/usr/bin/node")
        || trimmed.starts_with("#!/bin/node")
        || [
            "var ",
            "const ",
            "let ",
            "function ",
            "module.exports",
            "export ",
        ]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix))
    {
        return FileType::JavaScript;
    }

    if trimmed.starts_with("fn ")
        || trimmed.starts_with("pub ")
        || trimmed.starts_with("use ")
        || trimmed.starts_with("mod ")
        || trimmed.starts_with("impl ")
    {
        return FileType::Rust;
    }

    FileType::Unknown
}

#[cfg(test)]
mod tests {
    use super::{detect_file_type, FileType};

    #[test]
    fn detects_binary_magics() {
        assert_eq!(detect_file_type(b"MZ\x90\x00"), FileType::PE);
        assert_eq!(detect_file_type(b"\x7fELF"), FileType::ELF);
        assert_eq!(detect_file_type(b"\xfe\xed\xfa\xce"), FileType::MachO);
        assert_eq!(detect_file_type(b"PK\x03\x04"), FileType::Archive);
        assert_eq!(detect_file_type(b"\x89PNG\r\n\x1a\n"), FileType::Image);
    }

    #[test]
    fn detects_source_by_prefix() {
        assert_eq!(
            detect_file_type(b"#!/usr/bin/env python3\nprint('hi')"),
            FileType::Python
        );
        assert_eq!(
            detect_file_type(b"function main() { return 1; }"),
            FileType::JavaScript
        );
        assert_eq!(
            detect_file_type(b"use std::fs;\nfn main() {}"),
            FileType::Rust
        );
    }

    #[test]
    fn unknown_when_no_signal_matches() {
        assert_eq!(detect_file_type(b"random binary stuff"), FileType::Unknown);
    }
}
