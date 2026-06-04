//! Path-scoped `.gitignore` compilation (correct under ignored subtrees, deterministic).

use ignore::gitignore::{Gitignore, GitignoreBuilder};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_GITIGNORE_BYTES: u64 = 4 * 1024 * 1024;

/// Deepest directory whose `.gitignore` files participate in matching `path` (git semantics).
#[must_use]
pub(crate) fn terminal_dir_for_ignore_chain(
    walk_root: &Path,
    path: &Path,
    is_dir: bool,
) -> PathBuf {
    if is_dir {
        path.to_path_buf()
    } else {
        path.parent()
            .map_or_else(|| walk_root.to_path_buf(), Path::to_path_buf)
    }
}

/// Returns the git metadata directory (`.git` dir, or resolved `gitdir` for linked worktrees).
pub(crate) fn resolve_git_dir(start: &Path, walk_root: &Path) -> Option<PathBuf> {
    let mut cur = Some(start);
    while let Some(dir) = cur {
        let git = dir.join(".git");
        match std::fs::symlink_metadata(&git) {
            Ok(meta) if meta.is_dir() => return Some(git),
            Ok(meta) if meta.is_file() => {
                if let Some(resolved) = parse_gitdir_pointer(&git, dir, walk_root) {
                    return Some(resolved);
                }
            }
            _ => {}
        }
        cur = dir.parent();
    }
    None
}

fn parse_gitdir_pointer(
    git_file: &Path,
    containing_dir: &Path,
    walk_root: &Path,
) -> Option<PathBuf> {
    let text = std::fs::read_to_string(git_file).ok()?;
    for raw in text.lines() {
        let line = raw.trim();
        let Some(rest) = line.strip_prefix("gitdir:") else {
            continue;
        };
        let rest = rest.trim();
        let p = Path::new(rest);
        let resolved = if p.is_absolute() {
            p.to_path_buf()
        } else {
            containing_dir.join(p)
        };
        let resolved = resolved.canonicalize().ok().or(Some(resolved))?;
        if !resolved.starts_with(walk_root) {
            tracing::warn!(
                git_file = %git_file.display(),
                resolved = %resolved.display(),
                walk_root = %walk_root.display(),
                "gitdir pointer resolves outside walk root; ignoring"
            );
            return None;
        }
        return Some(resolved);
    }
    None
}

/// Returns the ordered list of directories whose `.gitignore` files should be consulted,
/// from the repository root (or `walk_root` when no repo is found) down to `terminal`.
pub(crate) fn ancestor_dirs_in_order(
    walk_root: &Path,
    terminal: &Path,
    repo_root: Option<&Path>,
) -> Vec<PathBuf> {
    let effective_root = repo_root
        .filter(|r| walk_root.starts_with(r) && *r != walk_root)
        .unwrap_or(walk_root);
    let mut dirs = vec![effective_root.to_path_buf()];
    if effective_root != walk_root {
        let Ok(rel) = walk_root.strip_prefix(effective_root) else {
            return dirs;
        };
        let mut cur = effective_root.to_path_buf();
        for c in rel.components() {
            cur.push(c);
            dirs.push(cur.clone());
        }
    }
    if terminal == walk_root {
        return dirs;
    }
    let Ok(rel) = terminal.strip_prefix(walk_root) else {
        return dirs;
    };
    let mut cur = walk_root.to_path_buf();
    for c in rel.components() {
        cur.push(c);
        dirs.push(cur.clone());
    }
    dirs
}

#[cfg(unix)]
pub(crate) fn append_one_gitignore(
    builder: &mut GitignoreBuilder,
    ignore_file: &Path,
) -> std::io::Result<()> {
    let sm = match std::fs::symlink_metadata(ignore_file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // Degrade gracefully: an unreadable .gitignore contributes no rules,
            // rather than aborting the walk and hiding its sibling files.
            tracing::warn!(path = %ignore_file.display(), error = %e, "permission denied reading .gitignore metadata; ignoring this file's rules");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    if sm.is_dir() {
        return Ok(());
    }
    // Match git(1): ignore files may be symlinks; read through to the target like a normal file.
    let file = match File::open(ignore_file) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // Degrade gracefully: an unreadable .gitignore contributes no rules,
            // rather than aborting the walk and hiding its sibling files.
            tracing::warn!(path = %ignore_file.display(), error = %e, "permission denied opening .gitignore; ignoring this file's rules");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let meta = file.metadata()?;
    if !meta.is_file() {
        return Ok(());
    }
    if meta.len() > MAX_GITIGNORE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Fix: .gitignore exceeds max size (4MiB); split or shrink the file.",
        ));
    }
    read_gitignore_lines_into_builder(builder, ignore_file, file)
}

#[cfg(not(unix))]
pub(crate) fn append_one_gitignore(
    builder: &mut GitignoreBuilder,
    ignore_file: &Path,
) -> std::io::Result<()> {
    let sm = match std::fs::symlink_metadata(ignore_file) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // Degrade gracefully: an unreadable .gitignore contributes no rules,
            // rather than aborting the walk and hiding its sibling files.
            tracing::warn!(path = %ignore_file.display(), error = %e, "permission denied reading .gitignore metadata; ignoring this file's rules");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    if sm.file_type().is_symlink() || !sm.is_file() {
        return Ok(());
    }
    let file = match File::open(ignore_file) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            // Degrade gracefully: an unreadable .gitignore contributes no rules,
            // rather than aborting the walk and hiding its sibling files.
            tracing::warn!(path = %ignore_file.display(), error = %e, "permission denied opening .gitignore; ignoring this file's rules");
            return Ok(());
        }
        Err(e) => return Err(e),
    };
    let meta = file.metadata()?;
    if meta.len() > MAX_GITIGNORE_BYTES {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "Fix: .gitignore exceeds max size (4MiB); split or shrink the file.",
        ));
    }
    read_gitignore_lines_into_builder(builder, ignore_file, file)
}

fn read_gitignore_lines_into_builder(
    builder: &mut GitignoreBuilder,
    ignore_file: &Path,
    file: File,
) -> std::io::Result<()> {
    let mut rdr = BufReader::new(file);
    let mut line_buf = String::new();
    let mut first = true;
    loop {
        line_buf.clear();
        let n = rdr.read_line(&mut line_buf)?;
        if n == 0 {
            break;
        }
        let line = if first {
            first = false;
            line_buf.trim_start_matches('\u{feff}')
        } else {
            line_buf.as_str()
        };
        let line = line.trim_end_matches('\r');
        if let Err(e) = builder.add_line(Some(ignore_file.to_path_buf()), line) {
            tracing::warn!(path = %ignore_file.display(), error = %e, "invalid .gitignore line");
        }
    }
    Ok(())
}

/// A node in a chained gitignore matcher. Each node holds the compiled rules from a single
/// `.gitignore` file and an optional link to the parent (shallower) node.
pub(crate) struct GitignoreNode {
    pub(crate) local: Gitignore,
    pub(crate) parent: Option<Arc<GitignoreNode>>,
}

impl GitignoreNode {
    /// Match `path` against this node and all ancestors, respecting git precedence rules
    /// (deepest match wins; whitelists override ignores).
    pub(crate) fn matched(&self, path: &Path, is_dir: bool) -> bool {
        let mut current = Some(self);
        while let Some(n) = current {
            let m = n.local.matched_path_or_any_parents(path, is_dir);
            if m.is_ignore() {
                return true;
            }
            if m.is_whitelist() {
                return false;
            }
            current = n.parent.as_deref();
        }
        false
    }
}

#[must_use]
pub(crate) fn path_is_ignored(node: &GitignoreNode, path: &Path, is_dir: bool) -> bool {
    node.matched(path, is_dir)
}
