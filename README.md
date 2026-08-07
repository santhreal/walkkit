# walkkit - Parallel filesystem walker with ignore-aware traversal

[![santh status](https://img.shields.io/badge/santh-stable-brightgreen)](https://santh.dev/standard)

High-performance parallel directory walker with ignore-aware traversal, bounded work queues,
and cycle detection. `walkkit` provides both a custom multi-threaded engine (`Walker`) with
glob filtering and inode sorting, and a codebase scanner (`CodeWalker`) tuned for workspace
analysis with `.gitignore` hierarchy parsing and lazy content loading.

All filesystem errors are preserved explicitly as typed error items rather than silently swallowed.
Binary files are detected via magic bytes and NUL-byte sampling, and symlink cycles are prevented
using OS directory identifiers.

## Quick Start

```rust
use walkkit::{Walker, WalkItem};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let walker = Walker::new()
        .add_root("./src")
        .with_parallelism(4)
        .respect_gitignore(true)
        .skip_binary(true);

    for item in walker.walk()? {
        match item {
            WalkItem::File(f) => println!("{}: {} bytes", f.path.display(), f.size),
            WalkItem::Error(e) => eprintln!("Error: {}", e),
        }
    }
    Ok(())
}
```

## When to use / when not to use

**When to use:**
- Parallel directory traversal over large codebases or large nested directories.
- Codebase scanning requiring `.gitignore` rules, glob include/exclude filters, and binary skipping.
- Applications needing strict error reporting without silent loss during traversal.

**When not to use:**
- Simple single-directory listings where standard `std::fs::read_dir` is sufficient.
- High-contention fine-grained parallel traversal across millions of tiny flat files where `jwalk` work-stealing is specifically needed.

## Compared to alternatives

`walkkit` provides structured parallel traversal with first-class `WalkItem::Error` propagation,
eliminating silent error suppression found in default iterators. Unlike `walkdir` (which is single-threaded),
`walkkit` leverages multi-threaded worker pools with bounded channels to bound in-flight memory.

Compared to `ignore` and `jwalk`, `walkkit` offers both a low-level bounded `Walker` and a high-level
`CodeWalker` abstraction, integrated with `hashkit` for content addressing and security probing.

## How it fits in Santh

`walkkit` lives in `libs/performance/io` as the foundational parallel directory discovery engine across
Santh analyzers, scanner tools, and indexing tools. It depends on `hashkit` for digest calculations
and provides the traversal primitive for codebase analysis pipelines.

## License

Dual-licensed under MIT OR Apache-2.0.
