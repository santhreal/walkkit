//! Decompression utilities for supported archive formats.
//!
//! Provides functions to decompress streams while enforcing strict memory limits
//! ([`MAX_DECOMPRESS_BYTES`]) to prevent zip bombs.

use super::entry::MAX_ENTRY_BYTES;
use super::error::ArchiveError;

/// Maximum decompressed size to prevent zip bombs (256MB).
pub const MAX_DECOMPRESS_BYTES: usize = MAX_ENTRY_BYTES;

/// Default maximum decompression ratio (output:input) to detect zip bombs.
pub const DEFAULT_MAX_DECOMPRESSION_RATIO: usize = 250;

/// Default total decompressed bytes budget across all extraction operations (1GB).
pub const DEFAULT_TOTAL_DECOMPRESSED_BUDGET: usize = 1024 * 1024 * 1024;

#[cfg(feature = "gzip")]
/// Decompress a gzip-compressed byte slice.
///
/// Ensures the decompressed data does not exceed [`MAX_DECOMPRESS_BYTES`].
///
/// # Example
///
/// ```
/// use archivewalk::decompress_gzip;
/// // Valid gzip data for an empty payload
/// let gzip_data = [
///     0x1f, 0x8b, 0x08, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00,
///     0x00, 0x00, 0x00, 0x00,
/// ];
/// let decompressed = decompress_gzip(&gzip_data).unwrap();
/// assert_eq!(decompressed.len(), 0);
/// ```
///
/// # Errors
/// Returns `ArchiveError` if decompression fails or if output exceeds [`MAX_DECOMPRESS_BYTES`].
pub fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, ArchiveError> {
    use std::io::Read;
    // Wire gpudeflate/nvcomp-sys here once nvcomp-sys is published.
    let mut decoder = flate2::read::GzDecoder::new(data);
    let mut out = Vec::with_capacity(data.len().min(1024 * 1024));
    let mut buf = vec![0u8; 65536];
    loop {
        let n = decoder
            .read(&mut buf)
            .map_err(|_| ArchiveError::InvalidHeader {
                offset: 0,
                message: "gzip decompression failed. Fix: verify the input is a complete, uncorrupted gzip stream.".into(),
            })?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() > MAX_DECOMPRESS_BYTES {
            return Err(ArchiveError::EntryTooLarge {
                size: out.len() as u64,
                max: MAX_DECOMPRESS_BYTES,
            });
        }
        let ratio_limit = data.len().saturating_mul(DEFAULT_MAX_DECOMPRESSION_RATIO);
        if ratio_limit > 0 && out.len() > ratio_limit {
            return Err(ArchiveError::EntryTooLarge {
                size: out.len() as u64,
                max: ratio_limit,
            });
        }
    }
    Ok(out)
}

#[cfg(feature = "zstd")]
/// Decompress a zstd-compressed byte slice.
///
/// Ensures the decompressed data does not exceed [`MAX_DECOMPRESS_BYTES`].
///
/// # Example
///
/// ```
/// use archivewalk::decompress_zstd;
/// // Valid empty zstd frame
/// let zstd_data = [0x28, 0xb5, 0x2f, 0xfd, 0x00, 0x58, 0x01, 0x00, 0x00];
/// let decompressed = decompress_zstd(&zstd_data).unwrap();
/// assert_eq!(decompressed.len(), 0);
/// ```
///
/// # Errors
///
/// Returns an error if decompression fails or exceeds [`MAX_DECOMPRESS_BYTES`].
pub fn decompress_zstd(data: &[u8]) -> Result<Vec<u8>, ArchiveError> {
    use std::io::Read;
    // Wire gpudeflate/nvcomp-sys here once nvcomp-sys is published.
    let mut decoder =
        zstd::stream::read::Decoder::new(data).map_err(|_| ArchiveError::InvalidHeader {
            offset: 0,
            message:
                "zstd decoder init failed. Fix: verify the input starts with a valid zstd frame."
                    .into(),
        })?;
    let mut out = Vec::with_capacity(data.len().min(1024 * 1024));
    let mut buf = vec![0u8; 65536];
    loop {
        let n = decoder.read(&mut buf).map_err(|_| ArchiveError::InvalidHeader {
            offset: 0,
            message: "zstd decompression failed. Fix: verify the frame is complete and not truncated.".into(),
        })?;
        if n == 0 {
            break;
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() > MAX_DECOMPRESS_BYTES {
            return Err(ArchiveError::EntryTooLarge {
                size: out.len() as u64,
                max: MAX_DECOMPRESS_BYTES,
            });
        }
        let ratio_limit = data.len().saturating_mul(DEFAULT_MAX_DECOMPRESSION_RATIO);
        if ratio_limit > 0 && out.len() > ratio_limit {
            return Err(ArchiveError::EntryTooLarge {
                size: out.len() as u64,
                max: ratio_limit,
            });
        }
    }
    Ok(out)
}
