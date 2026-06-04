use super::entry::{ArchiveEntry, MAX_ENTRIES, MAX_ENTRY_BYTES};
use super::error::ArchiveError;
use std::iter::FusedIterator;
use std::str;

/// Stateless ustar tar reader (in-memory slice).
///
/// # Example
///
/// Build tar bytes (or read from disk), then iterate:
///
/// ```no_run
/// use archivewalk::TarReader;
/// # let raw: &[u8] = &[];
/// let _entries: Vec<_> = TarReader::entries(raw).collect::<Result<Vec<_>, _>>().unwrap();
/// ```
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TarReader;

impl TarReader {
    /// Walk `data` as a tar stream and yield regular files (`typeflag` `0` or `'\0'`).
    ///
    /// Directories, hard links, symlinks, and special entries are skipped (consumed but not yielded).
    /// Symlink and hard-link entries still validate the `linkname` field for path traversal.
    /// End of archive requires **two** consecutive all-zero 512-byte blocks (POSIX); a single
    /// zero block with trailing data is rejected as corrupt, and a lone zero block at EOF is
    /// [`ArchiveError::Truncated`].
    #[must_use]
    pub fn entries(data: &[u8]) -> TarEntriesIter<'_> {
        TarEntriesIter {
            data,
            offset: 0,
            finished: false,
            entry_count: 0,
        }
    }
}

/// Iterator over tar regular files, yielding [`ArchiveEntry`] or [`ArchiveError`].
///
/// Stops after the first unrecoverable error. After two zero blocks (end-of-archive), iteration ends.
#[derive(Clone, Debug)]
pub struct TarEntriesIter<'a> {
    data: &'a [u8],
    offset: usize,
    finished: bool,
    entry_count: usize,
}

/// Checks if a path contains traversal patterns that could escape the extraction directory.
///
/// Returns `Err` if the path:
/// - Starts with '/' (absolute path)
/// - Contains '..' as a path component (parent directory traversal)
/// - Contains percent-encoded or unicode dot variants
/// # Errors
/// Returns `ArchiveError::PathTraversal` if the path contains `..`, encoded variants, or is absolute.
pub fn validate_path_traversal(name: &str) -> Result<(), ArchiveError> {
    // Reject paths with unicode dots that can be used for normalization bypass
    if name.chars().any(|c| {
        matches!(
            c,
            '\u{2024}' | '\u{2025}' | '\u{FE52}' | '\u{FF0E}' | '\u{3002}'
        )
    }) {
        return Err(ArchiveError::PathTraversal {
            name: name.to_string(),
        });
    }

    // Reject absolute paths
    if name.starts_with('/')
        || name.starts_with('\\')
        || name.starts_with('\u{FF0F}')
        || name.starts_with('\u{FF3C}')
    {
        return Err(ArchiveError::PathTraversal {
            name: name.to_string(),
        });
    }

    // Reject percent-encoded dot patterns that decode to ".."
    let lower = name.to_lowercase();
    let encoded_dotdot = ["%2e%2e", "%252e%252e", "%2e.", ".%2e", "%252e.", ".%252e"];
    if encoded_dotdot.iter().any(|pat| lower.contains(pat)) {
        return Err(ArchiveError::PathTraversal {
            name: name.to_string(),
        });
    }

    // Reject paths containing '..' as a normalized component
    for component in name.split(['/', '\\', '\u{FF0F}', '\u{FF3C}']) {
        let normalized = component.trim();
        if normalized == ".." {
            return Err(ArchiveError::PathTraversal {
                name: name.to_string(),
            });
        }
    }
    Ok(())
}

impl Iterator for TarEntriesIter<'_> {
    type Item = Result<ArchiveEntry, ArchiveError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        loop {
            if self.offset >= self.data.len() {
                self.finished = true;
                return None;
            }
            let header_off = self.offset;
            if self.data.len() - self.offset < 512 {
                self.finished = true;
                return Some(Err(ArchiveError::Truncated {
                    offset: header_off,
                    need: 512,
                    have: self.data.len() - header_off,
                }));
            }
            let header = &self.data[header_off..header_off + 512];
            if header.iter().all(|&b| b == 0) {
                // POSIX end-of-archive is two consecutive 512-byte zero blocks  -  not one.
                let Some(second_off) = header_off.checked_add(512) else {
                    self.finished = true;
                    return Some(Err(ArchiveError::InvalidHeader {
                        offset: header_off,
                        message:
                            "offset overflow after zero block. Fix: reject corrupted tar streams."
                                .into(),
                    }));
                };
                if second_off > self.data.len() {
                    self.finished = true;
                    return Some(Err(ArchiveError::Truncated {
                        offset: second_off,
                        need: 512,
                        have: 0,
                    }));
                }
                if self.data.len() - second_off < 512 {
                    self.finished = true;
                    return Some(Err(ArchiveError::Truncated {
                        offset: second_off,
                        need: 512,
                        have: self.data.len() - second_off,
                    }));
                }
                let second = &self.data[second_off..second_off + 512];
                if second.iter().all(|&b| b == 0) {
                    self.finished = true;
                    self.offset = second_off + 512;
                    return None;
                }
                self.finished = true;
                return Some(Err(ArchiveError::InvalidHeader {
                    offset: header_off,
                    message: "single zero block is not a valid ustar header (POSIX requires two zero blocks to end an archive). Fix: reject truncated or corrupted tar streams.".into(),
                }));
            }
            let parsed = match parse_header(header, header_off) {
                Ok(p) => p,
                Err(e) => {
                    self.finished = true;
                    return Some(Err(e));
                }
            };

            // Check for path traversal in the entry name
            if let Err(e) = validate_path_traversal(&parsed.full_name) {
                self.finished = true;
                return Some(Err(e));
            }

            // Check MAX_ENTRIES limit before processing this entry
            self.entry_count = self.entry_count.saturating_add(1);
            if self.entry_count > MAX_ENTRIES {
                self.finished = true;
                return Some(Err(ArchiveError::TooManyEntries {
                    count: self.entry_count,
                    max: MAX_ENTRIES,
                }));
            }

            let block_after_header = header_off + 512;

            // Check size against MAX_ENTRY_BYTES using checked arithmetic
            let size_usize = match parsed.size.try_into() {
                Ok(s) if s <= MAX_ENTRY_BYTES => s,
                _ => {
                    self.finished = true;
                    return Some(Err(ArchiveError::EntryTooLarge {
                        size: parsed.size,
                        max: MAX_ENTRY_BYTES,
                    }));
                }
            };

            // Compute padding using checked arithmetic
            let Some(pad) = padding_checked(parsed.size) else {
                self.finished = true;
                return Some(Err(ArchiveError::InvalidHeader {
                    offset: header_off,
                    message: "size overflow when computing padding. Fix: reject corrupted tar headers with implausibly large sizes.".into(),
                }));
            };

            let total_after = block_after_header
                .checked_add(size_usize)
                .and_then(|x| x.checked_add(pad));
            let Some(end) = total_after else {
                self.finished = true;
                return Some(Err(ArchiveError::InvalidHeader {
                    offset: header_off,
                    message: "size overflow when computing extent. Fix: reject corrupted tar headers with implausibly large sizes.".into(),
                }));
            };
            if end > self.data.len() {
                self.finished = true;
                return Some(Err(ArchiveError::Truncated {
                    offset: block_after_header,
                    need: size_usize + pad,
                    have: self.data.len().saturating_sub(block_after_header),
                }));
            }
            self.offset = end;
            if !parsed.is_regular_file {
                continue;
            }
            let payload = std::sync::Arc::from(
                &self.data[block_after_header..block_after_header + size_usize],
            );
            return Some(Ok(ArchiveEntry {
                name: parsed.full_name,
                size: parsed.size,
                data: payload,
            }));
        }
    }
}

impl FusedIterator for TarEntriesIter<'_> {}

struct ParsedHeader {
    full_name: String,
    size: u64,
    is_regular_file: bool,
}

fn parse_header(block: &[u8], offset: usize) -> Result<ParsedHeader, ArchiveError> {
    if block.len() != 512 {
        return Err(ArchiveError::InvalidHeader {
            offset,
            message: "internal header slice length must be 512. Fix: report this as a library bug; the iterator should always pass 512-byte blocks.".into(),
        });
    }
    if !checksum_valid(block) {
        return Err(ArchiveError::InvalidHeader {
            offset,
            message: "checksum mismatch. Fix: reject corrupted or modified tar archives.".into(),
        });
    }
    let name = field_str(&block[0..100]);
    let prefix = field_str(&block[345..500]);
    let full_name = join_path(prefix, name);
    let size = parse_octal_field(&block[124..136], "size", offset)?;
    let typeflag = block[156];
    let linkname = field_str(&block[157..257]);
    if typeflag == b'1' || typeflag == b'2' {
        if linkname.is_empty() {
            return Err(ArchiveError::InvalidHeader {
                offset,
                message: "hard link or symlink entry has an empty link path. Fix: reject corrupted tar headers.".into(),
            });
        }
        validate_path_traversal(&linkname)?;
    }
    let is_regular_file = typeflag == b'0' || typeflag == 0;
    Ok(ParsedHeader {
        full_name,
        size,
        is_regular_file,
    })
}

fn field_str(bytes: &[u8]) -> String {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let slice = &bytes[..end];
    String::from_utf8_lossy(trim_trailing_space(slice)).into_owned()
}

fn trim_trailing_space(s: &[u8]) -> &[u8] {
    let end = s
        .iter()
        .rposition(|b| *b != b' ' && *b != 0)
        .map_or(0, |i| i + 1);
    &s[..end]
}

fn join_path(prefix: String, name: String) -> String {
    if prefix.is_empty() {
        return name;
    }
    if name.is_empty() {
        return prefix;
    }
    let prefix_trim = prefix.trim_end_matches('/');
    let name_trim = name.trim_start_matches('/');
    format!("{prefix_trim}/{name_trim}")
}

fn parse_octal_field(field: &[u8], label: &str, offset: usize) -> Result<u64, ArchiveError> {
    let raw = field
        .iter()
        .copied()
        .take_while(|b| *b != 0 && *b != b' ')
        .collect::<Vec<u8>>();
    let s = str::from_utf8(&raw).map_err(|_| ArchiveError::InvalidHeader {
        offset,
        message: format!("{label} field is not valid UTF-8 for octal digits. Fix: ensure archive uses standard octal values."),
    })?;
    let s = s.trim();
    if s.is_empty() {
        return Ok(0);
    }
    u64::from_str_radix(s, 8).map_err(|err| match err.kind() {
        std::num::IntErrorKind::PosOverflow | std::num::IntErrorKind::NegOverflow => ArchiveError::InvalidHeader {
            offset,
            message: format!("{label} octal value {s:?} overflows u64. Fix: reject corrupted archives with oversized fields."),
        },
        _ => ArchiveError::InvalidHeader {
            offset,
            message: format!("{label} is not valid octal: {s:?}. Fix: check if the archive is corrupted or uses base-256 extensions."),
        }
    })
}

fn checksum_valid(block: &[u8]) -> bool {
    let mut sum: u32 = 0;
    for (i, &b) in block.iter().enumerate() {
        if (148..156).contains(&i) {
            sum += u32::from(b' ');
        } else {
            sum += u32::from(b);
        }
    }
    let Ok(stored) = parse_octal_field(&block[148..156], "checksum", 0) else {
        return false;
    };
    u64::from(sum) == stored
}

/// Computes padding with checked arithmetic to prevent 32-bit truncation.
/// Returns `None` if the calculation would overflow.
#[must_use]
pub fn padding_checked(size: u64) -> Option<usize> {
    let remainder = size % 512;
    let r: usize = remainder.try_into().ok()?;
    if r == 0 {
        Some(0)
    } else {
        Some(512 - r)
    }
}

#[cfg(test)]
mod tar_tests {
    use super::*;

    #[test]
    fn path_traversal_rejects_dotdot() {
        assert!(validate_path_traversal("../etc/passwd").is_err());
        assert!(validate_path_traversal("../../root/.ssh/id_rsa").is_err());
    }

    #[test]
    fn path_traversal_rejects_absolute() {
        assert!(validate_path_traversal("/etc/passwd").is_err());
    }

    #[test]
    fn path_traversal_accepts_normal() {
        assert!(validate_path_traversal("package/index.js").is_ok());
        assert!(validate_path_traversal("src/lib.rs").is_ok());
    }

    #[test]
    fn path_traversal_accepts_dotfile() {
        assert!(validate_path_traversal(".gitignore").is_ok());
        assert!(validate_path_traversal("src/.hidden").is_ok());
    }

    #[test]
    fn tar_entries_empty_data() {
        let entries: Vec<_> = TarReader::entries(&[]).collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn tar_entries_short_data_no_panic() {
        // Previously this was a FINDING  -  TarReader panicked on short data.
        // After investigation, the iterator returns Err(Truncated) which is correct.
        let results: Vec<_> = TarReader::entries(&[0u8; 100]).collect();
        // Should return 1 error (Truncated), not panic
        assert_eq!(
            results.len(),
            1,
            "short data should produce exactly 1 Truncated error"
        );
        assert!(results[0].is_err(), "result should be an error");
    }
}
