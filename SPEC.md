# walkkit  -  Specification

## Overview

`walkkit` is a Rust filesystem-walking library that provides two traversal engines: a custom multi-threaded `Walker` with bounded work queues, glob filtering, and explicit cycle detection, and a higher-level `CodeWalker` (backed by the `ignore` crate) tuned for codebase scanning with TOML-configurable ignore rules, extension filtering, and lazy content loading. Both engines support symlink handling, binary-file skipping, depth limits, and streaming or sorted result modes.

## Architecture

- **`lib.rs`**  -  Public exports and shared types (`WalkItem`, `WalkError`, `WalkOp`, `Error`).
- **`walker.rs`**  -  `Walker` builder, `WalkedFile` metadata struct, and platform-specific `DirId` for cycle detection.
- **`worker.rs`**  -  Single-threaded (`walk_single_thread`) and multi-threaded (`walk_multi_thread`) traversal workers. The multi-threaded worker uses a `Mutex<WorkState>` + `Condvar` shared queue with an `active` counter to detect termination.
- **`walk_common.rs`**  -  Helpers for metadata resolution, sorted directory enumeration (`read_dir_sorted`), binary probing via NUL-byte sampling (with TOCTOU guards on Unix), and the `WalkOptions` struct.
- **`filter.rs`**  -  `FileFilter` / `CompiledFilter` using `globset`, plus `code_entry_allowed` and `code_process_path` for the code walker.
- **`iter.rs`**  -  `WalkItemIter` adapter that yields from either a live `crossbeam_channel::Receiver` or a pre-collected buffered vector.
- **`sort.rs`**  -  `SortMode` (`Unsorted`, `ByName`, `BySize`).
- **`codewalker/`**  -  `traverse.rs` (`CodeWalker` using `ignore::WalkBuilder`), `parallel.rs` (`walk_parallel` with `ignore::WalkParallel`), and `mod.rs` (`WalkConfig`, `FileEntry`, `FileContent`, `FileContentChunks`).
- **`detect.rs`**  -  Binary detection via magic-byte signatures and extension heuristics.
- **`archive/`**  -  Feature-gated (`archive`) ZIP/TAR parsing and gzip/zstd decompression helpers.
- **`sandbox.rs`**  -  Async script execution helpers for security scanning workflows.

Data flow: `Walker::walk()` compiles filters, spawns a control thread, which launches worker threads that push `WalkItem` values through a bounded channel to the consuming iterator.

## Guarantees

- **No silent failures**  -  Every I/O error becomes a `WalkItem::Error` with the path and operation class preserved.
- **Symlink cycle detection**  -  When `follow_symlinks` is enabled, directories are tracked by stable `(dev, ino)` identifiers (Unix) or canonicalized paths (fallback) to prevent infinite loops.
- **Duplicate suppression**  -  `visited_files` and `visited_dirs` sets prevent re-emitting the same filesystem entity within a single walk.
- **Path length bounding**  -  Paths exceeding `MAX_WALK_PATH_BYTES` (8192) are rejected with an explicit error instead of being silently skipped.
- **TOCTOU hardening**  -  On Unix, binary probing opens files with `O_NOFOLLOW` and verifies `ino`/`dev` match the prior `stat` before reading.

## Public API

### `Walker` (custom parallel walker)
- `Walker::new()`  -  Builder with default parallelism (4).
- `.add_root(path)`  -  Add a traversal root.
- `.with_parallelism(n)`  -  Set worker count (clamped to `[1, available_parallelism * 4]` capped at 256).
- `.with_filter(FileFilter)`  -  Glob include/exclude rules.
- `.with_sort(SortMode)`  -  `Unsorted`, `ByName`, or `BySize`.
- `.follow_symlinks(bool)`, `.respect_gitignore(bool)`, `.skip_binary(bool)`
- `.with_max_depth(d)`, `.with_extension_filter(ext)`, `.with_size_limit(bytes)`
- `.walk() -> Result<WalkItemIter, Error>`  -  Streaming or buffered iteration.
- `.try_walk_parallel() -> Result<Receiver<WalkItem>, Error>`  -  Direct channel access.

### `CodeWalker` (codebase scanner)
- `CodeWalker::new(root, WalkConfig)`  -  TOML-deserializable config with defaults for `skip_binary`, `skip_hidden`, `respect_gitignore`, `exclude_dirs`, `max_file_size`, etc.
- `.walk() -> Result<Vec<FileEntry>>`  -  Collect all entries.
- `.walk_sorted() -> Result<Vec<FileEntry>>`  -  Sorted by path.
- `.walk_iter() -> impl Iterator<Item = Result<FileEntry>>`  -  Lazy iteration.
- `.walk_parallel(threads) -> mpsc::Receiver<Result<FileEntry>>`  -  Parallel traversal via `ignore::WalkParallel`.

### Supporting types
- `WalkItem`  -  `File(WalkedFile)` or `Error(WalkError)`.
- `WalkedFile`  -  `{ path: PathBuf, size: u64, inode: u64 }`.
- `FileEntry`  -  `{ path: PathBuf, size: u64, is_binary: bool }` with `.content()`, `.content_chunks()`, `.content_str()`.
- `FileContent`  -  `Text(String)`, `Binary(Vec<u8>)`, or `Unknown(Vec<u8>)`.
- `FileFilter` / `CompiledFilter`  -  Builder + compiled glob matcher.

## Error handling

- **`WalkError`**  -  Non-fatal traversal errors (`Metadata`, `ReadDir`, `Open`, `Gitignore`). Always emitted as `WalkItem::Error`; the walk continues elsewhere when possible.
- **`Error`**  -  Fatal configuration errors from `Walker::walk()`: invalid glob syntax (`InvalidGlob`) or empty/NUL-containing filter patterns (`InvalidFilterPattern`).
- **`CodewalkError`**  -  Errors from `CodeWalker`: `Io`, `FileTooLarge(u64)`, `Ignore`, `Utf8Error`.

## Performance characteristics

- **Traversal**  -  `O(N)` in the number of directory entries visited; multi-threaded scaling is bounded by the coarse mutex-protected work queue (not work-stealing).
- **Memory**  -  Streaming mode (`SortMode::Unsorted`) holds only the bounded channel (1024 items) in flight. Sorted modes buffer all results, requiring `O(M)` memory for `M` matched files.
- **Binary detection**  -  First 8 KB read for files ≤ 10 MB; strided 64 KB samples for larger files. Magic-byte checks run before opening the file when the extension is in a known-binary list.
- **Directory enumeration**  -  Entries are sorted lexicographically by `OsStr` bytes to improve determinism; this adds `O(K log K)` per directory with `K` children.

## Limitations

- `Walker` does not use a work-stealing deque; high contention on the shared mutex can limit scaling on very large, shallow directories.
- `CodeWalker` relies on the `ignore` crate for traversal; its parallelism and gitignore semantics are inherited from that dependency.
- Binary detection is heuristic (magic bytes, extension list, and NUL-byte sampling) and may misclassify unusual text or binary formats.
- `Walker` deduplicates files by `PathBuf`, not by inode, so the same file reached via hard links under different names may be emitted multiple times.
- No async/`await` API for `Walker`; it spawns OS threads internally.
- `MAX_WALK_PATH_BYTES` is hard-coded to 8192; platforms with shorter `PATH_MAX` may still encounter OS-level errors before this bound is reached.
- The `archive` feature provides parsing helpers but does not integrate automatic archive expansion into the main `Walker` or `CodeWalker` traversal.
