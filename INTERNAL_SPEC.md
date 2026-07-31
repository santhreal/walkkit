# walkkit  -  Internal Spec

> This file is gitignored. It exists for agents and internal development. Never committed to public repos.

## Identity
Parallel filesystem walker with ignore-aware traversal and bounded work queues.

## Purpose
Provides high-performance parallel directory traversal with filtering, gitignore support, sorting, and safe symlink handling. Without it, Santh has no way to discover files at internet scale.

## North Star
Be the fastest, most correct, and most configurable directory walker in the Rust ecosystem  -  the one ripgrep and similar tools would choose.

## Role in Ecosystem
- **Depends on:** hashkit (internal)
- **Depended on by:** archivewalk, netshift, manifestkit, vyre, yaragpu, ziftsieve, fusedpipe, codewalk, surges, warpgrep, warpscan, warpscan-ingest
- **Relationship to warpscan:** Core file discovery engine used by warpscan and warpscan-ingest to enumerate target packages and source trees.
- **Standalone value:** YES  -  any Rust project needing fast parallel directory walking with ignore rules can use it independently.

## Invariants
- Symlinks are never followed by default.
- Every I/O error surfaces as `WalkItem::Error`; errors are never swallowed.
- Path lengths are capped at `MAX_WALK_PATH_BYTES`; `ENAMETOOLONG` is reported with an actionable hint.
- Gitignore rules are respected when the `gitignore` feature is enabled.
- Archive feature flags (`gzip`, `zstd`, `zip-deflate`) are additive and do not change base walker behavior.

## Boundaries
- Does not parse archive contents on its own (archivewalk provides the compatibility facade).
- Does not hash file contents beyond optional MD5.
- Does not manage network I/O.
- Does not execute user-defined code during traversal.

## Quality State
- Tests: 20+ declared test targets (unit, adversarial, concurrent, property, integration, regression, depth)
- Lint preamble: yes
- #![forbid(unsafe_code)]: yes
- Doc coverage: ~90%
- Known issues: None from latest audit
