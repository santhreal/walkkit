//! File tree walker  -  the core iteration engine.

mod parallel;
mod traverse;

pub use traverse::CodeWalker;

use std::collections::HashSet;
use std::io::Read;
use std::path::PathBuf;

const DEFAULT_MAX_SYMLINK_DEPTH: usize = 16;
pub(crate) const READ_CHUNK_SIZE: usize = 64 * 1024;

/// Configuration for directory walking.
///
/// Loadable from TOML:
/// ```toml
/// max_file_size = 10485760  # 10 MB
/// skip_binary = true
/// skip_hidden = true
/// respect_gitignore = true
/// follow_symlinks = false
/// include_extensions = ["rs", "py", "js"]
/// exclude_dirs = ["node_modules", ".git", "target"]
/// ```
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
#[allow(clippy::struct_excessive_bools)]
pub struct WalkConfig {
    /// Maximum file size in bytes to include (0 = unlimited).
    pub max_file_size: u64,
    /// Skip files detected as binary.
    pub skip_binary: bool,
    /// Skip hidden files and directories (dotfiles).
    pub skip_hidden: bool,
    /// Respect `.gitignore` rules.
    pub respect_gitignore: bool,
    /// Follow symbolic links while walking.
    pub follow_symlinks: bool,
    /// Only include files with these extensions (empty = all).
    pub include_extensions: HashSet<String>,
    /// Exclude files with these extensions.
    pub exclude_extensions: HashSet<String>,
    /// Directories to always skip.
    pub exclude_dirs: HashSet<String>,
    /// Custom ignore files to respect (e.g. `[".keyhogignore"]`).
    pub ignore_files: Vec<String>,
    /// Global ignore patterns (overrides).
    pub ignore_patterns: Vec<String>,
    /// Maximum number of symlink hops allowed in any discovered path.
    pub max_symlink_depth: usize,
}

impl Default for WalkConfig {
    fn default() -> Self {
        let exclude_dirs: HashSet<String> = [
            "node_modules",
            ".git",
            "target",
            "__pycache__",
            ".venv",
            "venv",
            ".tox",
            ".mypy_cache",
            ".pytest_cache",
            "dist",
            "build",
            ".next",
            ".nuxt",
            "vendor",
            ".bundle",
            ".gradle",
            ".mvn",
            "Pods",
        ]
        .iter()
        .map(std::string::ToString::to_string)
        .collect();

        Self {
            max_file_size: 10 * 1024 * 1024, // 10 MB
            skip_binary: true,
            skip_hidden: true,
            respect_gitignore: true,
            follow_symlinks: false,
            include_extensions: HashSet::new(),
            exclude_extensions: HashSet::new(),
            exclude_dirs,
            ignore_files: vec![".keyhogignore".to_string()],
            ignore_patterns: Vec::new(),
            max_symlink_depth: DEFAULT_MAX_SYMLINK_DEPTH,
        }
    }
}

impl WalkConfig {
    /// Create a new default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a fluent builder starting from sensible defaults.
    #[must_use]
    pub fn builder() -> Self {
        Self::default()
    }

    /// Create defaults tuned for extracted artifacts and scanner workspaces.
    #[must_use]
    pub fn artifact_defaults() -> Self {
        Self {
            skip_hidden: false,
            respect_gitignore: false,
            exclude_dirs: HashSet::new(),
            ..Self::default()
        }
    }

    /// Load configuration from a TOML file.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or contains invalid TOML.
    pub fn load(path: &std::path::Path) -> Result<Self, Box<dyn std::error::Error>> {
        let content = std::fs::read_to_string(path)?;
        let config: WalkConfig = toml::from_str(&content)?;
        Ok(config)
    }

    /// Parse configuration from a TOML string.
    ///
    /// # Errors
    /// Returns an error if the string contains invalid TOML.
    pub fn from_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(toml_str)
    }

    /// Set maximum file size in bytes.
    #[must_use]
    pub fn max_file_size(mut self, max_file_size: u64) -> Self {
        self.max_file_size = max_file_size;
        self
    }

    /// Configure whether binary files are skipped.
    #[must_use]
    pub fn skip_binary(mut self, skip_binary: bool) -> Self {
        self.skip_binary = skip_binary;
        self
    }


    /// Configure whether hidden files are skipped.
    #[must_use]
    pub fn skip_hidden(mut self, skip_hidden: bool) -> Self {
        self.skip_hidden = skip_hidden;
        self
    }

    /// Configure `.gitignore` handling.
    #[must_use]
    pub fn respect_gitignore(mut self, respect_gitignore: bool) -> Self {
        self.respect_gitignore = respect_gitignore;
        self
    }

    /// Configure symbolic link traversal.
    #[must_use]
    pub fn follow_symlinks(mut self, follow_symlinks: bool) -> Self {
        self.follow_symlinks = follow_symlinks;
        self
    }

    /// Set extensions to include.
    #[must_use]
    pub fn include_extensions(mut self, include_extensions: HashSet<String>) -> Self {
        self.include_extensions = include_extensions;
        self
    }

    /// Set extensions to exclude.
    #[must_use]
    pub fn exclude_extensions(mut self, exclude_extensions: HashSet<String>) -> Self {
        self.exclude_extensions = exclude_extensions;
        self
    }

    /// Set directories to always skip.
    #[must_use]
    pub fn exclude_dirs(mut self, exclude_dirs: HashSet<String>) -> Self {
        self.exclude_dirs = exclude_dirs;
        self
    }

    /// Set custom ignore files.
    #[must_use]
    pub fn ignore_files(mut self, ignore_files: Vec<String>) -> Self {
        self.ignore_files = ignore_files;
        self
    }

    /// Set global ignore patterns.
    #[must_use]
    pub fn ignore_patterns(mut self, ignore_patterns: Vec<String>) -> Self {
        self.ignore_patterns = ignore_patterns;
        self
    }

    /// Set the maximum number of symlink hops to follow per path.
    #[must_use]
    pub fn max_symlink_depth(mut self, max_symlink_depth: usize) -> Self {
        self.max_symlink_depth = max_symlink_depth;
        self
    }
}

/// A discovered file entry with lazy content loading.
#[derive(Clone, Debug)]
pub struct FileEntry {
    /// Absolute path to the file.
    pub path: PathBuf,
    /// File size in bytes.
    pub size: u64,
    /// Whether the file was detected as binary.
    pub is_binary: bool,
}

impl FileEntry {
    /// Read the file content.
    ///
    /// # Errors
    /// Returns an error if the file cannot be opened or read.
    pub fn content(&self) -> crate::error::Result<FileContent> {
        const MAX_CONTENT_AUTOLOAD: u64 = 256 * 1024 * 1024;

        // Re-stat the live file so that growth after the directory walk does
        // not cause silent truncation. The cached size is only used as a hint
        // for the initial allocation; the actual read proceeds to EOF.
        let metadata = std::fs::metadata(&self.path)?;
        let live_size = metadata.len();
        if live_size > MAX_CONTENT_AUTOLOAD {
            return Err(crate::error::CodewalkError::FileTooLarge(live_size));
        }

        let bounded_capacity = usize::try_from(live_size).unwrap_or(READ_CHUNK_SIZE);
        let mut bytes = Vec::with_capacity(bounded_capacity);

        // Read straight into `bytes` in one pass. `read_to_end` grows this single
        // buffer by amortized doubling; the old loop over `content_chunks()`
        // allocated a fresh 64 KiB `Vec` on every chunk and copied it in.
        // `take(MAX + 1)` bounds the read so a file that grew past the cap after
        // the walk is detected and rejected here rather than silently truncated,
        // while never buffering more than MAX + 1 bytes.
        let file = std::fs::File::open(&self.path)?;
        let read = file.take(MAX_CONTENT_AUTOLOAD + 1).read_to_end(&mut bytes)?;
        if read as u64 > MAX_CONTENT_AUTOLOAD {
            return Err(crate::error::CodewalkError::FileTooLarge(read as u64));
        }

        if self.is_binary {
            return Ok(FileContent::Binary(bytes));
        }

        match String::from_utf8(bytes) {
            Ok(text) => Ok(FileContent::Text(text)),
            Err(err) => Ok(FileContent::Unknown(err.into_bytes())),
        }
    }

    /// Read the file content incrementally in bounded chunks.
    ///
    /// # Errors
    /// Returns an error if the file cannot be opened.
    pub fn content_chunks(&self) -> crate::error::Result<FileContentChunks> {
        Ok(FileContentChunks {
            file: std::fs::File::open(&self.path)?,
            done: false,
        })
    }

    /// Read the file content as a UTF-8 string.
    ///
    /// # Errors
    /// Returns an error if the file cannot be read or contains invalid UTF-8.
    pub fn content_str(&self) -> crate::error::Result<String> {
        match self.content()? {
            FileContent::Text(text) => Ok(text),
            FileContent::Binary(bytes) | FileContent::Unknown(bytes) => {
                Ok(String::from_utf8(bytes)?)
            }
        }
    }
}

/// File content loaded from disk.
#[derive(Debug)]
#[non_exhaustive]
pub enum FileContent {
    /// UTF-8 text content.
    Text(String),
    /// Content that was classified as binary during the walk.
    Binary(Vec<u8>),
    /// Content that was not classified as binary but was not valid UTF-8.
    Unknown(Vec<u8>),
}

impl std::fmt::Display for FileContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Text(_) => "text",
            Self::Binary(_) => "binary",
            Self::Unknown(_) => "unknown",
        })
    }
}

impl FileContent {
    /// Get the content as a byte slice.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Text(text) => text.as_bytes(),
            Self::Binary(bytes) | Self::Unknown(bytes) => bytes.as_slice(),
        }
    }

    /// Return the text content when the file decoded as UTF-8.
    #[must_use]
    pub fn as_text(&self) -> Option<&str> {
        match self {
            Self::Text(text) => Some(text.as_str()),
            Self::Binary(_) | Self::Unknown(_) => None,
        }
    }

    /// Whether the content is text.
    #[must_use]
    pub fn is_text(&self) -> bool {
        matches!(self, Self::Text(_))
    }

    /// Whether the content was classified as binary.
    #[must_use]
    pub fn is_binary(&self) -> bool {
        matches!(self, Self::Binary(_))
    }

    /// Whether the content bytes could not be decoded as UTF-8.
    #[must_use]
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown(_))
    }

    /// Content length.
    #[must_use]
    pub fn len(&self) -> usize {
        self.as_bytes().len()
    }

    /// Whether the content is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl AsRef<[u8]> for FileContent {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

/// Iterator over a file's content in bounded chunks.
#[derive(Debug)]
pub struct FileContentChunks {
    file: std::fs::File,
    done: bool,
}

impl Iterator for FileContentChunks {
    type Item = crate::error::Result<Vec<u8>>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        let mut chunk = vec![0_u8; READ_CHUNK_SIZE];
        match self.file.read(&mut chunk) {
            Ok(0) => {
                self.done = true;
                None
            }
            Ok(read) => {
                chunk.truncate(read);
                Some(Ok(chunk))
            }
            Err(err) => {
                self.done = true;
                Some(Err(err.into()))
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod test_utils {
    #![allow(clippy::unwrap_used)]
    use std::fs;
    pub(crate) fn setup_test_dir() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
        fs::write(dir.path().join("lib.rs"), "pub fn hello() {}").unwrap();
        fs::write(dir.path().join("data.bin"), b"\x7fELF\x00\x00\x00\x00").unwrap();
        fs::create_dir(dir.path().join("node_modules")).unwrap();
        fs::write(dir.path().join("node_modules/junk.js"), "// junk").unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/app.py"), "print('hello')").unwrap();
        dir
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use std::fs;

    #[test]
    fn file_content_read() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("test.txt"), "hello world").unwrap();

        let config = WalkConfig {
            skip_binary: false,
            ..WalkConfig::default()
        };
        let walker = CodeWalker::new(dir.path(), config);
        let entries = walker.walk().unwrap();
        assert_eq!(entries.len(), 1);

        let content = entries[0].content().unwrap();
        assert_eq!(content.as_bytes(), b"hello world");
        assert_eq!(content.len(), 11);
        assert!(!content.is_empty());
    }

    #[test]
    fn file_content_read_larger_than_prealloc_cap_is_exact() {
        // A file larger than the old 256 KiB (READ_CHUNK_SIZE * 4) pre-alloc cap
        // must read back byte-exact across the multi-chunk path. This exercises
        // the branch that previously reallocated repeatedly and now pre-allocates
        // the full known size in one shot.
        let dir = tempfile::tempdir().unwrap();
        let size = READ_CHUNK_SIZE * 5 + 123; // 320 KiB + 123, spans several chunks
        let mut expected = Vec::with_capacity(size);
        // A non-repeating, non-UTF8-ambiguous byte pattern so a chunk-boundary
        // off-by-one would corrupt the round-trip and fail the assert.
        for i in 0..size {
            expected.push((i % 251) as u8);
        }
        fs::write(dir.path().join("big.bin"), &expected).unwrap();

        let config = WalkConfig {
            skip_binary: false,
            ..WalkConfig::default()
        };
        let walker = CodeWalker::new(dir.path(), config);
        let entries = walker.walk().unwrap();
        assert_eq!(entries.len(), 1);

        let content = entries[0].content().unwrap();
        assert_eq!(content.len(), size);
        assert_eq!(content.as_bytes(), &expected[..]);
    }

    #[test]
    fn file_content_reads_exact_chunk_multiple_boundary() {
        // A file whose size is an EXACT multiple of READ_CHUNK_SIZE is the case
        // most likely to expose an off-by-one at the read boundary. The single
        // `read_to_end` path (which replaced the per-chunk-allocating loop) must
        // return every byte with no truncation and no trailing over-read.
        let dir = tempfile::tempdir().unwrap();
        let size = READ_CHUNK_SIZE * 3; // exactly three chunks, no remainder
        let expected: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        fs::write(dir.path().join("aligned.bin"), &expected).unwrap();

        let config = WalkConfig {
            skip_binary: false,
            ..WalkConfig::default()
        };
        let walker = CodeWalker::new(dir.path(), config);
        let entries = walker.walk().unwrap();
        assert_eq!(entries.len(), 1);

        let content = entries[0].content().unwrap();
        assert_eq!(content.len(), size, "aligned file must read fully, no truncation");
        assert_eq!(content.as_bytes(), &expected[..]);
    }

    #[test]
    fn file_content_reads_grown_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("grows.txt");
        fs::write(&path, "small").unwrap();

        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let entries = walker.walk().unwrap();
        assert_eq!(entries.len(), 1);

        // File grows after the directory walk; content() must read the full
        // current bytes rather than silently stopping at the cached size.
        fs::write(&path, "this is the full grown content").unwrap();

        let content = entries[0].content().unwrap();
        assert_eq!(content.as_bytes(), b"this is the full grown content");
    }

    #[test]
    fn file_content_str() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("test.rs"), "fn main() {}").unwrap();

        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let entries = walker.walk().unwrap();
        let s = entries[0].content_str().unwrap();
        assert_eq!(s, "fn main() {}");
    }

    #[test]
    fn default_config_excludes_common_dirs() {
        let config = WalkConfig::default();
        assert!(config.exclude_dirs.contains("node_modules"));
        assert!(config.exclude_dirs.contains(".git"));
        assert!(config.exclude_dirs.contains("target"));
        assert!(config.exclude_dirs.contains("__pycache__"));
        assert!(config.exclude_dirs.contains("vendor"));
    }
}
