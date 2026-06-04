# walkkit

Parallel file discovery with filtering, sorting, and depth control.

## Quick Start

```rust
use walkkit::Walker;

let files: Vec<_> = Walker::new()
    .add_root("./src")
    .with_parallelism(4)
    .respect_gitignore(true)
    .skip_binary(true)
    .walk()
    .collect();
```

## Features

- Parallel multi-threaded walking
- Glob include/exclude filtering
- Inode sorting for sequential disk access
- .gitignore support (skips .git directories)
- Binary file detection (NUL byte check)
- Depth limiting
- Symlink cycle detection
- Extension and size filters

## License

MIT
