# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [0.1.1] - 2026-07-30

### Fixed
- `FileEntry::content` no longer silently truncates files that grow between
  `walk()` and `content()`: it re-stats the live file, reads to EOF, and
  fails closed with `FileTooLarge` above the 256 MiB autoload cap.
- `FileEntry::content` now reads into a single pre-sized buffer instead of
  allocating and copying a fresh 64 KiB `Vec` per chunk.

## [0.1.0] - 2025-04-12

### Added
- Initial release of `walkkit`.
- `Walker`  -  parallel multi-threaded filesystem walker with bounded work queues.
- `CodeWalker`  -  codebase scanner built on the `ignore` crate with lazy content loading.
- Glob-based include/exclude filtering via `FileFilter` and `CompiledFilter`.
- Gitignore-aware traversal with per-directory `.gitignore` chain compilation and caching.
- Binary file detection via magic bytes, extension heuristics, and NUL-byte probing.
- Configurable sorting modes (`Unsorted`, `ByName`, `BySize`).
- Depth limiting, extension filtering, and size-limit filtering.
- Symlink cycle detection using platform-specific directory identifiers (`DirId`).
- TOCTOU-hardened binary probing on Unix (`O_NOFOLLOW` + `(dev, ino)` validation).
- `WalkConfig` with TOML serialization and sensible defaults for source-tree scanning.
- Optional archive support behind the `archive` feature (gzip, zstd, zip-deflate).
