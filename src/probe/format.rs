//! Compression and archive/container format detection from leading bytes.

const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];
const ZSTD_MAGIC: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];
const LZ4_FRAME_MAGIC: [u8; 4] = [0x04, 0x22, 0x4d, 0x18];
const ZIP_LOCAL_MAGIC: [u8; 4] = [0x50, 0x4b, 0x03, 0x04];
const ZIP_EMPTY_MAGIC: [u8; 4] = [0x50, 0x4b, 0x05, 0x06];
const SNAPPY_FRAMED_MAGIC: [u8; 10] = [0xff, 0x06, 0x00, 0x00, b's', b'N', b'a', b'P', b'p', b'Y'];
const TAR_USTAR_OFFSET: usize = 257;
const TAR_USTAR_MAGIC: &[u8; 6] = b"ustar\0";

/// Recognized compression or outer-container format.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
#[non_exhaustive]
pub enum DecompressFormat {
    /// Format could not be recognized from the bytes provided.
    Unknown,
    /// Auto-detect from magic bytes.
    Auto,
    /// Raw gzip stream.
    Gzip,
    /// Raw zstd frame.
    Zstd,
    /// LZ4 frame.
    Lz4,
    /// Snappy framed stream.
    Snappy,
    /// ZIP archive.
    Zip,
    /// POSIX ustar tar stream.
    Tar,
    /// Gzip-wrapped tar. Not returned by [`detect_format`] from outer magic alone.
    TarGz,
}

impl DecompressFormat {
    /// Return a stable lowercase identifier for diagnostics.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Auto => "auto",
            Self::Gzip => "gzip",
            Self::Zstd => "zstd",
            Self::Lz4 => "lz4",
            Self::Snappy => "snappy",
            Self::Zip => "zip",
            Self::Tar => "tar",
            Self::TarGz => "tar.gz",
        }
    }
}

/// Detect a compression or archive/container format from raw bytes.
#[must_use]
pub fn detect_format(data: &[u8]) -> DecompressFormat {
    if data.len() >= ZIP_LOCAL_MAGIC.len() {
        if data[..ZIP_LOCAL_MAGIC.len()] == ZIP_LOCAL_MAGIC
            || data[..ZIP_EMPTY_MAGIC.len()] == ZIP_EMPTY_MAGIC
        {
            return DecompressFormat::Zip;
        }
        if data[..ZSTD_MAGIC.len()] == ZSTD_MAGIC {
            return DecompressFormat::Zstd;
        }
        if data[..LZ4_FRAME_MAGIC.len()] == LZ4_FRAME_MAGIC {
            return DecompressFormat::Lz4;
        }
    }

    if data.len() >= GZIP_MAGIC.len() && data[..GZIP_MAGIC.len()] == GZIP_MAGIC {
        return DecompressFormat::Gzip;
    }

    if data.len() >= SNAPPY_FRAMED_MAGIC.len()
        && data[..SNAPPY_FRAMED_MAGIC.len()] == SNAPPY_FRAMED_MAGIC
    {
        return DecompressFormat::Snappy;
    }

    if data.len() >= TAR_USTAR_OFFSET + TAR_USTAR_MAGIC.len()
        && data[TAR_USTAR_OFFSET..TAR_USTAR_OFFSET + TAR_USTAR_MAGIC.len()] == *TAR_USTAR_MAGIC
    {
        return DecompressFormat::Tar;
    }

    DecompressFormat::Unknown
}

#[cfg(test)]
mod tests {
    use super::{detect_format, DecompressFormat};

    #[test]
    fn detects_common_compression_magic_bytes() {
        assert_eq!(detect_format(&[0x1f, 0x8b, 0x08]), DecompressFormat::Gzip);
        assert_eq!(
            detect_format(&[0x28, 0xb5, 0x2f, 0xfd, 0x00]),
            DecompressFormat::Zstd
        );
        assert_eq!(
            detect_format(&[0x04, 0x22, 0x4d, 0x18, 0x00]),
            DecompressFormat::Lz4
        );
        assert_eq!(
            detect_format(&[0xff, 0x06, 0x00, 0x00, b's', b'N', b'a', b'P', b'p', b'Y']),
            DecompressFormat::Snappy
        );
    }

    #[test]
    fn detects_zip_and_tar() {
        assert_eq!(
            detect_format(&[0x50, 0x4b, 0x03, 0x04, 0x14, 0x00]),
            DecompressFormat::Zip
        );

        let mut tar = vec![0u8; 300];
        tar[257..263].copy_from_slice(b"ustar\0");
        assert_eq!(detect_format(&tar), DecompressFormat::Tar);
    }

    #[test]
    fn unknown_when_magic_does_not_match() {
        assert_eq!(detect_format(&[]), DecompressFormat::Unknown);
        assert_eq!(detect_format(&[0x00]), DecompressFormat::Unknown);
        assert_eq!(
            detect_format(&[0xDE, 0xAD, 0xBE, 0xEF]),
            DecompressFormat::Unknown
        );
    }
}
