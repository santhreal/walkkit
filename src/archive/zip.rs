use super::entry::{MAX_ENTRIES, MAX_ENTRY_BYTES};
use super::error::ArchiveError;
use super::tar::validate_path_traversal;

/// Parses the central directory to locate files, then reads their local headers
/// and payload data. Stored entries keep their raw bytes in `data`. Deflated
/// entries keep their compressed bytes in `data`; [`auto_extract`] inflates them.
///
/// # Example
///
/// ```no_run
/// use archivewalk::ZipReader;
/// # let zip_bytes: &[u8] = &[];
/// let entries = ZipReader::entries(zip_bytes).unwrap();
/// for entry in &entries {
///     println!("{}: {} bytes", entry.name, entry.size);
/// }
/// ```
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ZipReader;

/// Compression method for a ZIP entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ZipCompression {
    /// Stored (no compression)  -  `data` contains raw file bytes.
    Stored,
    /// Deflated  -  `data` contains compressed bytes that must be decompressed.
    Deflated,
    /// Unknown compression method.
    Other(u16),
}

/// A single file extracted from a ZIP archive.
#[derive(Clone, Debug)]
pub struct ZipEntry {
    /// Path inside the archive.
    pub name: String,
    /// Uncompressed file size.
    pub size: u64,
    /// Compression method used.
    pub compression: ZipCompression,
    /// File content (raw bytes for Stored, compressed bytes for Deflated). Ref-counted.
    pub data: std::sync::Arc<[u8]>,
}

impl PartialEq for ZipEntry {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.size == other.size
            && self.compression == other.compression
            && *self.data == *other.data
    }
}

impl Eq for ZipEntry {}

/// ZIP32 fields set to this value indicate Zip64 extra data; we do not parse Zip64.
const ZIP32_SIZE_SENTINEL: u32 = 0xFFFF_FFFF;

fn reject_zip64_field(pos: usize, label: &str, raw: u32) -> Result<(), ArchiveError> {
    if raw == ZIP32_SIZE_SENTINEL {
        return Err(ArchiveError::InvalidHeader {
            offset: pos,
            message: format!(
                "{label} uses 0xFFFFFFFF (Zip64). Fix: use a non-Zip64 archive or add Zip64 support."
            ),
        });
    }
    Ok(())
}

impl ZipReader {
    /// Parse ZIP entries from an in-memory slice.
    ///
    /// Reads the end-of-central-directory record (last 22+ bytes), then parses
    /// central directory entries, and finally reads local file headers + data.
    ///
    /// # Errors
    ///
    /// Returns an error if the ZIP is truncated or structurally invalid.
    ///
    /// Entry names must be valid UTF-8, match between the central directory and local header as
    /// raw bytes, and pass the same path-traversal rules as tar ([`super::tar::validate_path_traversal`]).
    pub fn entries(data: &[u8]) -> Result<Vec<ZipEntry>, ArchiveError> {
        let eocd = find_eocd(data)?;
        let cd_offset = usize::try_from(eocd.cd_offset).map_err(|_| ArchiveError::InvalidHeader {
            offset: 0,
            message: "central directory offset does not fit usize. Fix: use a smaller archive or a 64-bit platform.".into(),
        })?;
        let entry_count = eocd.entry_count as usize;

        if entry_count > MAX_ENTRIES {
            return Err(ArchiveError::TooManyEntries {
                count: entry_count,
                max: MAX_ENTRIES,
            });
        }

        let mut entries = Vec::with_capacity(entry_count);
        let mut pos = cd_offset;

        for _ in 0..entry_count {
            let pos_after_header = pos.checked_add(46).ok_or(ArchiveError::InvalidHeader {
                offset: pos,
                message: "central directory offset overflow. Fix: reject ZIPs with malformed structural offsets (possible corruption or attack).".into(),
            })?;
            if pos_after_header > data.len() {
                return Err(ArchiveError::Truncated {
                    offset: pos,
                    need: 46,
                    have: data.len().saturating_sub(pos),
                });
            }

            let sig = read_u32_le(data, pos);
            if sig != 0x0201_4B50 {
                return Err(ArchiveError::InvalidHeader {
                    offset: pos,
                    message: format!(
                        "expected central directory signature 0x02014B50, got {sig:#010x}. Fix: verify the input is a valid ZIP archive or reject truncated/corrupted data."
                    ),
                });
            }

            let compression_raw = read_u16_le(data, pos + 10);
            let compressed_raw = read_u32_le(data, pos + 20);
            let uncompressed_raw = read_u32_le(data, pos + 24);
            let local_offset_raw = read_u32_le(data, pos + 42);
            reject_zip64_field(pos + 20, "compressed size", compressed_raw)?;
            reject_zip64_field(pos + 24, "uncompressed size", uncompressed_raw)?;
            reject_zip64_field(pos + 42, "local header offset", local_offset_raw)?;
            let compressed_size = compressed_raw as usize;
            let uncompressed_size = u64::from(uncompressed_raw);
            let name_len = read_u16_le(data, pos + 28) as usize;
            let extra_len = read_u16_le(data, pos + 30) as usize;
            let comment_len = read_u16_le(data, pos + 32) as usize;
            let local_header_offset = local_offset_raw as usize;

            let name_end = pos_after_header
                .checked_add(name_len)
                .ok_or(ArchiveError::InvalidHeader {
                    offset: pos,
                    message: "central directory name length overflow. Fix: reject ZIPs with malformed length fields (possible corruption or attack).".into(),
                })?;
            if name_end > data.len() {
                return Err(ArchiveError::Truncated {
                    offset: pos_after_header,
                    need: name_len,
                    have: data.len().saturating_sub(pos_after_header),
                });
            }
            let cd_name_bytes = &data[pos_after_header..name_end];
            let name = std::str::from_utf8(cd_name_bytes).map_err(|_| ArchiveError::InvalidHeader {
                offset: pos_after_header,
                message: "ZIP entry name is not valid UTF-8. Fix: only UTF-8 entry names are supported.".into(),
            })?;
            let name = name.to_owned();

            // Validate path traversal
            validate_path_traversal(&name)?;

            let compression = match compression_raw {
                0 => ZipCompression::Stored,
                8 => ZipCompression::Deflated,
                other => ZipCompression::Other(other),
            };

            // Skip directories (name ends with '/')
            if !name.ends_with('/') {
                // Read from local file header
                let local_header_end = local_header_offset
                    .checked_add(30)
                    .ok_or(ArchiveError::InvalidHeader {
                        offset: local_header_offset,
                        message: "local file header offset overflow. Fix: reject ZIPs with malformed structural offsets (possible corruption or attack).".into(),
                    })?;
                if local_header_end > data.len() {
                    return Err(ArchiveError::Truncated {
                        offset: local_header_offset,
                        need: 30,
                        have: data.len().saturating_sub(local_header_offset),
                    });
                }

                let local_sig = read_u32_le(data, local_header_offset);
                if local_sig != 0x0403_4B50 {
                    return Err(ArchiveError::InvalidHeader {
                        offset: local_header_offset,
                        message: "invalid local file header signature. Fix: verify the input is a valid ZIP archive or reject truncated/corrupted data.".into(),
                    });
                }

                let local_name_len = read_u16_le(data, local_header_offset + 26) as usize;
                let local_extra_len = read_u16_le(data, local_header_offset + 28) as usize;

                let local_name_start = local_header_end;
                let local_name_end = local_name_start
                    .checked_add(local_name_len)
                    .ok_or(ArchiveError::InvalidHeader {
                        offset: local_header_offset,
                        message: "local file header name length overflow. Fix: reject ZIPs with malformed length fields (possible corruption or attack).".into(),
                    })?;
                if local_name_end > data.len() {
                    return Err(ArchiveError::Truncated {
                        offset: local_name_start,
                        need: local_name_len,
                        have: data.len().saturating_sub(local_name_start),
                    });
                }
                let local_name = &data[local_name_start..local_name_end];
                if local_name != cd_name_bytes {
                    return Err(ArchiveError::InvalidHeader {
                        offset: local_header_offset,
                        message: "local file name bytes do not match central directory. Fix: reject ZIPs with mismatched CD/local names (possible mixed-encoding or zip-slip attempt).".into(),
                    });
                }

                let payload_start = local_name_end
                    .checked_add(local_extra_len)
                    .ok_or(ArchiveError::InvalidHeader {
                        offset: local_header_offset,
                        message: "local file header extra length overflow. Fix: reject ZIPs with malformed length fields (possible corruption or attack).".into(),
                    })?;

                if compressed_size > MAX_ENTRY_BYTES {
                    return Err(ArchiveError::EntryTooLarge {
                        size: compressed_size as u64,
                        max: MAX_ENTRY_BYTES,
                    });
                }

                if compression == ZipCompression::Stored {
                    if compressed_size as u64 != uncompressed_size {
                        return Err(ArchiveError::InvalidHeader {
                            offset: pos,
                            message: format!(
                                "stored ZIP entry declares uncompressed size {uncompressed_size} but on-disk payload is {compressed_size} bytes (must match for method STORED). Fix: reject corrupted or malicious ZIP fields."
                            ),
                        });
                    }
                    if uncompressed_size > MAX_ENTRY_BYTES as u64 {
                        return Err(ArchiveError::EntryTooLarge {
                            size: uncompressed_size,
                            max: MAX_ENTRY_BYTES,
                        });
                    }
                }

                let payload_end = payload_start
                    .checked_add(compressed_size)
                    .ok_or(ArchiveError::InvalidHeader {
                        offset: local_header_offset,
                        message: "compressed size overflow. Fix: reject ZIPs with malformed size fields (possible corruption or attack).".into(),
                    })?;
                if payload_end > data.len() {
                    return Err(ArchiveError::Truncated {
                        offset: payload_start,
                        need: compressed_size,
                        have: data.len().saturating_sub(payload_start),
                    });
                }

                let file_data = std::sync::Arc::from(&data[payload_start..payload_end]);

                entries.push(ZipEntry {
                    name,
                    size: uncompressed_size,
                    compression,
                    data: file_data,
                });
            }

            pos = name_end
                .checked_add(extra_len)
                .and_then(|v| v.checked_add(comment_len))
                .ok_or(ArchiveError::InvalidHeader {
                    offset: pos,
                    message: "central directory entry extent overflow. Fix: reject ZIPs with malformed structural offsets (possible corruption or attack).".into(),
                })?;
        }

        Ok(entries)
    }
}

/// End-of-central-directory record.
struct Eocd {
    entry_count: u16,
    cd_offset: u32,
}

/// Find the EOCD record by scanning backwards from the end of the data.
fn find_eocd(data: &[u8]) -> Result<Eocd, ArchiveError> {
    // EOCD is at least 22 bytes and has signature 0x06054B50.
    // It can be preceded by a variable-length comment, so scan backwards.
    if data.len() < 22 {
        return Err(ArchiveError::InvalidHeader {
            offset: 0,
            message: "ZIP file too small for EOCD record. Fix: pass the complete ZIP archive or reject truncated data.".into(),
        });
    }

    let max_comment = 65535.min(data.len() - 22);
    for i in 0..=max_comment {
        let pos = data.len() - 22 - i;
        if read_u32_le(data, pos) == 0x0605_4B50 {
            let entry_count = read_u16_le(data, pos + 10);
            let cd_offset = read_u32_le(data, pos + 16);
            if entry_count == u16::MAX {
                return Err(ArchiveError::InvalidHeader {
                    offset: pos,
                    message: "ZIP uses Zip64 end-of-central-directory (65535 entries). Fix: use a smaller archive or implement Zip64.".into(),
                });
            }
            if cd_offset == ZIP32_SIZE_SENTINEL {
                return Err(ArchiveError::InvalidHeader {
                    offset: pos,
                    message: "ZIP central directory offset is 0xFFFFFFFF (Zip64). Fix: use a non-Zip64 archive or add Zip64 support.".into(),
                });
            }
            return Ok(Eocd {
                entry_count,
                cd_offset,
            });
        }
    }

    Err(ArchiveError::InvalidHeader {
        offset: 0,
        message: "EOCD signature not found. Fix: verify the input is a valid ZIP archive or reject truncated/corrupted data.".into(),
    })
}

fn read_u16_le(data: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([data[offset], data[offset + 1]])
}

fn read_u32_le(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}
