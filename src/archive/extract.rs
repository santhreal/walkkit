//! High-level extraction wrappers.
//!
//! Handles auto-detection, decompression, and recursive nested archive unpacking safely.
use super::decompress::DEFAULT_MAX_DECOMPRESSION_RATIO;
#[cfg(feature = "zip-deflate")]
use super::decompress::MAX_DECOMPRESS_BYTES;
#[cfg(any(feature = "gzip", feature = "zstd"))]
use super::decompress::{decompress_gzip, decompress_zstd};
use super::entry::ArchiveEntry;
use super::error::ArchiveError;
use super::tar::TarReader;
use super::zip::{ZipCompression, ZipEntry, ZipReader};
use crate::probe::format::{detect_format, DecompressFormat as ArchiveFormat};

/// Auto-detect format, decompress if needed, and extract tar entries.
///
/// Handles: plain tar, tar.gz, tar.zst. Returns entries from the inner tar.
///
/// # Example
///
/// ```no_run
/// use walkkit::archive::auto_extract;
/// # let raw_bytes: &[u8] = &[];
/// let entries = auto_extract(raw_bytes).unwrap();
/// ```
///
/// # Errors
///
/// Returns an error if format detection, decompression, or tar parsing fails.
pub fn auto_extract(data: &[u8]) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let mut budget = DecompressBudget::new();
    auto_extract_with_budget(data, &mut budget)
}

fn auto_extract_with_budget(
    data: &[u8],
    budget: &mut DecompressBudget,
) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    match detect_format(data) {
        ArchiveFormat::Tar => {
            let entries: Vec<ArchiveEntry> =
                TarReader::entries(data).collect::<Result<Vec<_>, _>>()?;
            for entry in &entries {
                budget.add(entry.data.len())?;
            }
            Ok(entries)
        }
        #[cfg(feature = "gzip")]
        ArchiveFormat::Gzip => {
            let decompressed = decompress_gzip(data)?;
            budget.add(decompressed.len())?;
            TarReader::entries(&decompressed).collect()
        }
        #[cfg(feature = "zstd")]
        ArchiveFormat::Zstd => {
            let decompressed = decompress_zstd(data)?;
            budget.add(decompressed.len())?;
            TarReader::entries(&decompressed).collect()
        }
        ArchiveFormat::Zip => {
            let zip_entries = ZipReader::entries(data)?;
            Ok(zip_entries
                .into_iter()
                .map(|e| zip_entry_into_archive_entry(e, budget))
                .collect::<Result<Vec<_>, _>>()?)
        }
        _ => Err(ArchiveError::InvalidHeader {
            offset: 0,
            message: "unrecognized archive format. Fix: provide a supported archive format (tar, zip, gzip, zstd).".into(),
        }),
    }
}

fn zip_entry_into_archive_entry(
    entry: ZipEntry,
    budget: &mut DecompressBudget,
) -> Result<ArchiveEntry, ArchiveError> {
    let ZipEntry {
        name,
        size,
        compression,
        data,
    } = entry;

    let data = match compression {
        ZipCompression::Stored => {
            budget.add(data.len())?;
            data
        }
        ZipCompression::Deflated => {
            let decompressed = decompress_zip_deflate(&data, size)?;
            budget.add(decompressed.len())?;
            std::sync::Arc::from(decompressed)
        }
        ZipCompression::Other(method) => {
            return Err(ArchiveError::InvalidHeader {
                offset: 0,
                message: format!(
                    "unsupported ZIP compression method {method}. Fix: use stored/deflated entries or add a decoder."
                ),
            });
        }
    };

    Ok(ArchiveEntry { name, size, data })
}

#[cfg(feature = "zip-deflate")]
fn decompress_zip_deflate(data: &[u8], expected_size: u64) -> Result<Vec<u8>, ArchiveError> {
    use std::io::Read;

    // Wire gpudeflate/nvcomp-sys here once nvcomp-sys is published.
    let mut decoder = flate2::read::DeflateDecoder::new(data);
    let mut out = Vec::with_capacity(expected_size.min(MAX_DECOMPRESS_BYTES as u64) as usize);
    let mut buf = vec![0u8; 65536];
    let ratio_limit = data.len().saturating_mul(DEFAULT_MAX_DECOMPRESSION_RATIO);
    loop {
        let read = decoder
            .read(&mut buf)
            .map_err(|_| ArchiveError::InvalidHeader {
                offset: 0,
                message: "ZIP deflate decompression failed. Fix: verify the compressed data is complete and uncorrupted.".into(),
            })?;
        if read == 0 {
            break;
        }
        out.extend_from_slice(&buf[..read]);
        if out.len() > MAX_DECOMPRESS_BYTES {
            return Err(ArchiveError::EntryTooLarge {
                size: out.len() as u64,
                max: MAX_DECOMPRESS_BYTES,
            });
        }
        if ratio_limit > 0 && out.len() > ratio_limit {
            return Err(ArchiveError::EntryTooLarge {
                size: out.len() as u64,
                max: ratio_limit,
            });
        }
    }

    if out.len() as u64 != expected_size {
        return Err(ArchiveError::InvalidHeader {
            offset: 0,
            message: format!(
                "ZIP deflate size mismatch: expected {expected_size} bytes, decoded {} bytes",
                out.len()
            ),
        });
    }

    Ok(out)
}

#[cfg(not(feature = "zip-deflate"))]
fn decompress_zip_deflate(_data: &[u8], _expected_size: u64) -> Result<Vec<u8>, ArchiveError> {
    Err(ArchiveError::InvalidHeader {
        offset: 0,
        message: "ZIP deflate support is disabled. Fix: enable the `zip-deflate` feature.".into(),
    })
}

/// Auto-detect archives recursively up to `max_depth`.
///
/// Nested archive entry names are prefixed with their parent archive path using
/// `/` separators.
///
/// # Example
///
/// ```no_run
/// use walkkit::archive::auto_extract_recursive;
/// # let raw_bytes: &[u8] = &[];
/// let entries = auto_extract_recursive(raw_bytes, 3).unwrap();
/// ```
///
/// # Errors
///
/// Returns an error if any archive layer fails to extract.
/// Maximum total output bytes across all recursive extraction layers.
const MAX_RECURSIVE_OUTPUT_BYTES: usize = 512 * 1024 * 1024; // 512 MB
/// Maximum total entries across all recursive extraction layers.
const MAX_RECURSIVE_ENTRIES: usize = 100_000;
/// Hard maximum recursion depth to prevent stack overflow.
const MAX_RECURSIVE_DEPTH: usize = 10;

/// Recursively extract nested archives up to `max_depth` levels deep.
///
/// Applies global budgets to prevent zip-bomb expansion:
/// - Maximum 512 MB total output
/// - Maximum 100,000 entries
///
/// # Errors
///
/// Returns an error if any archive layer fails to extract or budgets are exceeded.
pub fn auto_extract_recursive(
    data: &[u8],
    max_depth: usize,
) -> Result<Vec<ArchiveEntry>, ArchiveError> {
    let max_depth = max_depth.min(MAX_RECURSIVE_DEPTH);
    let mut out = Vec::new();
    let mut total_bytes: usize = 0;
    let mut budget = DecompressBudget::new();
    extract_recursive_into(data, max_depth, "", &mut out, &mut total_bytes, &mut budget)?;
    Ok(out)
}

/// Tracks cumulative decompressed bytes across nested extraction operations.
struct DecompressBudget {
    total_decompressed: usize,
}

impl DecompressBudget {
    fn new() -> Self {
        Self {
            total_decompressed: 0,
        }
    }

    fn add(&mut self, bytes: usize) -> Result<(), ArchiveError> {
        self.total_decompressed = self.total_decompressed.checked_add(bytes).ok_or_else(|| {
            ArchiveError::InvalidHeader {
                offset: 0,
                message:
                    "total decompressed byte count overflowed. Fix: reject this archive as hostile."
                        .into(),
            }
        })?;
        if self.total_decompressed > super::decompress::DEFAULT_TOTAL_DECOMPRESSED_BUDGET {
            return Err(ArchiveError::InvalidHeader {
                offset: 0,
                message: format!(
                    "total decompressed bytes exceeded {} bytes ({} GB). Fix: reject archives with excessive cumulative decompression (zip bomb).",
                    super::decompress::DEFAULT_TOTAL_DECOMPRESSED_BUDGET,
                    super::decompress::DEFAULT_TOTAL_DECOMPRESSED_BUDGET / (1024 * 1024 * 1024)
                ),
            });
        }
        Ok(())
    }
}

fn extract_recursive_into(
    data: &[u8],
    max_depth: usize,
    prefix: &str,
    out: &mut Vec<ArchiveEntry>,
    total_bytes: &mut usize,
    budget: &mut DecompressBudget,
) -> Result<(), ArchiveError> {
    let entries = auto_extract_with_budget(data, budget)?;
    for entry in entries {
        // Budget check: total entries
        if out.len() >= MAX_RECURSIVE_ENTRIES {
            return Err(ArchiveError::InvalidHeader {
                offset: 0,
                message: format!(
                    "recursive extraction exceeded {MAX_RECURSIVE_ENTRIES} entries  -  possible zip bomb"
                ),
            });
        }

        // Budget check: total output bytes (checked to avoid silent saturation surprises)
        *total_bytes = total_bytes
            .checked_add(entry.data.len())
            .ok_or(ArchiveError::InvalidHeader {
                offset: 0,
                message: format!(
                    "recursive extraction total byte budget overflowed usize. Fix: reject this archive as hostile."
                ),
            })?;
        if *total_bytes > MAX_RECURSIVE_OUTPUT_BYTES {
            return Err(ArchiveError::InvalidHeader {
                offset: 0,
                message: format!(
                    "recursive extraction exceeded {} MB total output  -  possible zip bomb",
                    MAX_RECURSIVE_OUTPUT_BYTES / (1024 * 1024)
                ),
            });
        }

        let name = if prefix.is_empty() {
            entry.name.clone()
        } else {
            format!("{prefix}/{}", entry.name)
        };

        if max_depth > 0 && detect_format(&entry.data) != ArchiveFormat::Unknown {
            extract_recursive_into(&entry.data, max_depth - 1, &name, out, total_bytes, budget)?;
        } else {
            out.push(ArchiveEntry {
                name,
                size: entry.size,
                data: entry.data,
            });
        }
    }
    Ok(())
}
