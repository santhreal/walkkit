//! Parallel and single-threaded traversal workers (queue, cycle detection, gitignore hooks).

use crate::filter::CompiledFilter;
use crate::walk_common::{
    build_walked_file, is_unresolvable_symlink, lock_work_state, metadata_for_path,
    read_dir_sorted, wait_for_work, ReadDirSorted, WalkOptions, WorkState,
};
use crate::walker::{dir_id, path_exceeds_walk_limit};
use crate::{WalkError, WalkItem, WalkOp};
use crossbeam_channel::Sender;
#[cfg(feature = "gitignore")]
use ignore::gitignore::GitignoreBuilder;
use std::path::{Path, PathBuf};
use std::thread;

#[cfg(feature = "gitignore")]
use crate::gitignore_ctx::{
    ancestor_dirs_in_order, append_one_gitignore, path_is_ignored, resolve_git_dir,
    terminal_dir_for_ignore_chain, GitignoreNode,
};
#[cfg(feature = "gitignore")]
use std::sync::{Arc, Mutex};

#[cfg(feature = "gitignore")]
type GitignoreCache = Mutex<std::collections::HashMap<(PathBuf, PathBuf), Arc<GitignoreNode>>>;

#[cfg(feature = "gitignore")]
fn get_cached_gitignore(
    cache: &GitignoreCache,
    walk_root: &Path,
    path: &Path,
    is_dir: bool,
) -> Result<Arc<GitignoreNode>, WalkError> {
    let terminal = terminal_dir_for_ignore_chain(walk_root, path, is_dir);
    let key = (walk_root.to_path_buf(), terminal.clone());
    {
        let guard = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(g) = guard.get(&key) {
            return Ok(Arc::clone(g));
        }
    }

    let repo_root = resolve_git_dir(walk_root, walk_root)
        .and_then(|git_dir| git_dir.parent().map(Path::to_path_buf));
    let dirs = ancestor_dirs_in_order(walk_root, &terminal, repo_root.as_deref());

    let mut parent: Option<Arc<GitignoreNode>> = None;
    for d in &dirs {
        let dir_key = (walk_root.to_path_buf(), d.clone());
        {
            let guard = cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(node) = guard.get(&dir_key) {
                parent = Some(Arc::clone(node));
                continue;
            }
        }
        let mut builder = GitignoreBuilder::new(walk_root);
        let gi = d.join(".gitignore");
        let built = (|| -> std::io::Result<ignore::gitignore::Gitignore> {
            append_one_gitignore(&mut builder, &gi)?;
            builder.build().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Fix: repair .gitignore glob syntax. {e}"),
                )
            })
        })();
        let local = match built {
            Ok(g) => g,
            Err(e) => return Err(WalkError::new(gi, WalkOp::Gitignore, e)),
        };
        let node = Arc::new(GitignoreNode { local, parent });
        {
            let mut guard = cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            guard.insert(dir_key, Arc::clone(&node));
            parent = Some(node);
        }
    }

    if let Some(git_dir) = resolve_git_dir(walk_root, walk_root) {
        let exclude = git_dir.join("info/exclude");
        let mut builder = GitignoreBuilder::new(walk_root);
        let built = (|| -> std::io::Result<ignore::gitignore::Gitignore> {
            append_one_gitignore(&mut builder, &exclude)?;
            builder.build().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Fix: repair .gitignore glob syntax. {e}"),
                )
            })
        })();
        let local = match built {
            Ok(g) => g,
            Err(e) => return Err(WalkError::new(exclude, WalkOp::Gitignore, e)),
        };
        parent = Some(Arc::new(GitignoreNode { local, parent }));
    }

    let node = parent.unwrap_or_else(|| {
        Arc::new(GitignoreNode {
            local: ignore::gitignore::Gitignore::empty(),
            parent: None,
        })
    });
    {
        let mut guard = cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        guard.insert(key, Arc::clone(&node));
    }
    Ok(node)
}

#[cfg(feature = "gitignore")]
fn gitignore_for_path_local(
    cache: &mut std::collections::HashMap<(PathBuf, PathBuf), Arc<GitignoreNode>>,
    walk_root: &Path,
    path: &Path,
    is_dir: bool,
) -> Result<Arc<GitignoreNode>, WalkError> {
    let terminal = terminal_dir_for_ignore_chain(walk_root, path, is_dir);
    let key = (walk_root.to_path_buf(), terminal.clone());
    if let Some(g) = cache.get(&key) {
        return Ok(Arc::clone(g));
    }

    let repo_root = resolve_git_dir(walk_root, walk_root)
        .and_then(|git_dir| git_dir.parent().map(Path::to_path_buf));
    let dirs = ancestor_dirs_in_order(walk_root, &terminal, repo_root.as_deref());

    let mut parent: Option<Arc<GitignoreNode>> = None;
    for d in &dirs {
        let dir_key = (walk_root.to_path_buf(), d.clone());
        if let Some(node) = cache.get(&dir_key) {
            parent = Some(Arc::clone(node));
            continue;
        }
        let mut builder = GitignoreBuilder::new(walk_root);
        let gi = d.join(".gitignore");
        let built = (|| -> std::io::Result<ignore::gitignore::Gitignore> {
            append_one_gitignore(&mut builder, &gi)?;
            builder.build().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Fix: repair .gitignore glob syntax. {e}"),
                )
            })
        })();
        let local = match built {
            Ok(g) => g,
            Err(e) => return Err(WalkError::new(gi, WalkOp::Gitignore, e)),
        };
        let node = Arc::new(GitignoreNode { local, parent });
        cache.insert(dir_key, Arc::clone(&node));
        parent = Some(node);
    }

    if let Some(git_dir) = resolve_git_dir(walk_root, walk_root) {
        let exclude = git_dir.join("info/exclude");
        let mut builder = GitignoreBuilder::new(walk_root);
        let built = (|| -> std::io::Result<ignore::gitignore::Gitignore> {
            append_one_gitignore(&mut builder, &exclude)?;
            builder.build().map_err(|e| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("Fix: repair .gitignore glob syntax. {e}"),
                )
            })
        })();
        let local = match built {
            Ok(g) => g,
            Err(e) => return Err(WalkError::new(exclude, WalkOp::Gitignore, e)),
        };
        parent = Some(Arc::new(GitignoreNode { local, parent }));
    }

    let node = parent.unwrap_or_else(|| {
        Arc::new(GitignoreNode {
            local: ignore::gitignore::Gitignore::empty(),
            parent: None,
        })
    });
    cache.insert(key, Arc::clone(&node));
    Ok(node)
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn walk_single_thread(
    roots: Vec<PathBuf>,
    filter: &CompiledFilter,
    options: &WalkOptions,
    tx: &Sender<WalkItem>,
) {
    use std::collections::VecDeque;

    #[cfg(feature = "gitignore")]
    let mut gi_cache: std::collections::HashMap<(PathBuf, PathBuf), Arc<GitignoreNode>> =
        std::collections::HashMap::new();

    let mut queue: VecDeque<(PathBuf, usize, PathBuf)> = VecDeque::new();
    let mut visited_dirs: std::collections::HashSet<crate::walker::DirId> =
        std::collections::HashSet::new();
    let mut visited_files: std::collections::HashSet<PathBuf> = std::collections::HashSet::new();

    for root in roots {
        if path_exceeds_walk_limit(&root) {
            let _ = tx.send(WalkItem::Error(WalkError::new(
                root.clone(),
                WalkOp::Metadata,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "Fix: shorten path to ≤{} bytes (walkkit MAX_WALK_PATH_BYTES).",
                        crate::walker::MAX_WALK_PATH_BYTES
                    ),
                ),
            )));
            continue;
        }
        let wr = root.clone();
        queue.push_back((root, 0usize, wr));
    }

    while let Some((path, depth, walk_root)) = queue.pop_front() {
        if path_exceeds_walk_limit(&path) {
            let _ = tx.send(WalkItem::Error(WalkError::new(
                path.clone(),
                WalkOp::Metadata,
                std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!(
                        "Fix: shorten path to ≤{} bytes (walkkit MAX_WALK_PATH_BYTES).",
                        crate::walker::MAX_WALK_PATH_BYTES
                    ),
                ),
            )));
            continue;
        }

        let meta = match metadata_for_path(&path, options.follow_symlinks) {
            Ok(m) => m,
            Err(e) => {
                if is_unresolvable_symlink(&path, options.follow_symlinks) {
                    continue;
                }
                let _ = tx.send(WalkItem::Error(WalkError::new(
                    path.clone(),
                    WalkOp::Metadata,
                    e,
                )));
                continue;
            }
        };

        #[cfg(feature = "gitignore")]
        let is_ignored = if options.respect_gitignore {
            match gitignore_for_path_local(&mut gi_cache, &walk_root, &path, meta.is_dir()) {
                Ok(gi) => path_is_ignored(&gi, &path, meta.is_dir()),
                Err(e) => {
                    let _ = tx.send(WalkItem::Error(e));
                    continue;
                }
            }
        } else {
            false
        };

        #[cfg(not(feature = "gitignore"))]
        let is_ignored = false;

        if is_ignored {
            continue;
        }

        if meta.is_dir() {
            if options.respect_gitignore && path.file_name().is_some_and(|n| n == ".git") {
                continue;
            }

            let dir_key = match dir_id(&path, &meta, options.follow_symlinks) {
                Ok(k) => k,
                Err(e) => {
                    let _ = tx.send(WalkItem::Error(WalkError::new(
                        path.clone(),
                        WalkOp::Metadata,
                        e,
                    )));
                    continue;
                }
            };
            if !visited_dirs.insert(dir_key) {
                tracing::debug!(path = %path.display(), "skipping directory: cycle detected");
                continue;
            }

            let recurse = options.max_depth.is_none_or(|max| depth <= max);
            if recurse {
                match read_dir_sorted(&path, options.max_dir_entries) {
                    Ok(ReadDirSorted {
                        paths: children,
                        entry_errors,
                    }) => {
                        for e in entry_errors {
                            let _ = tx.send(WalkItem::Error(WalkError::new(
                                path.clone(),
                                WalkOp::ReadDir,
                                e,
                            )));
                        }
                        for child_path in children {
                            if path_exceeds_walk_limit(&child_path) {
                                let _ = tx.send(WalkItem::Error(WalkError::new(
                                    child_path.clone(),
                                    WalkOp::Metadata,
                                    std::io::Error::new(
                                        std::io::ErrorKind::InvalidInput,
                                        format!(
                                            "Fix: shorten path to ≤{} bytes (walkkit MAX_WALK_PATH_BYTES).",
                                            crate::walker::MAX_WALK_PATH_BYTES
                                        ),
                                    ),
                                )));
                                continue;
                            }
                            queue.push_back((child_path, depth + 1, walk_root.clone()));
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(WalkItem::Error(WalkError::new(
                            path.clone(),
                            WalkOp::ReadDir,
                            e,
                        )));
                    }
                }
            }
        } else {
            match build_walked_file(
                path.clone(),
                &meta,
                filter,
                options.extension_filter.as_deref(),
                options.size_limit,
                options.skip_binary,
                options.follow_symlinks,
            ) {
                Ok(Some(file)) => {
                    if !visited_files.insert(path) {
                        continue;
                    }
                    if tx.send(WalkItem::File(file)).is_err() {
                        break;
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    let _ = tx.send(WalkItem::Error(e));
                }
            }
        }
    }
}

fn signal_done_locked(st: &mut WorkState, cv: &std::sync::Condvar) {
    st.active -= 1;
    if st.active == 0 && st.queue.is_empty() {
        cv.notify_all();
    }
}

fn dec_active(state: &std::sync::Arc<(std::sync::Mutex<WorkState>, std::sync::Condvar)>) {
    let mut st = lock_work_state(&state.0);
    signal_done_locked(&mut st, &state.1);
}

#[allow(clippy::too_many_lines)]
#[allow(clippy::needless_pass_by_value)]
pub(crate) fn walk_multi_thread(
    roots: Vec<PathBuf>,
    filter: CompiledFilter,
    tx: &Sender<WalkItem>,
    parallelism: usize,
    options: WalkOptions,
) {
    #[cfg(feature = "gitignore")]
    let gi_cache: Arc<GitignoreCache> = Arc::new(Mutex::new(std::collections::HashMap::new()));

    let shared_state = std::sync::Arc::new((
        std::sync::Mutex::new(WorkState {
            active: 0,
            queue: roots
                .into_iter()
                .map(|path| (path.clone(), 0usize, path, None))
                .collect(),
            visited_dirs: std::collections::HashSet::new(),
            visited_files: std::collections::HashSet::new(),
        }),
        std::sync::Condvar::new(),
    ));

    let filter = std::sync::Arc::new(filter);
    let options = std::sync::Arc::new(options);
    let mut workers = Vec::new();

    for _ in 0..parallelism {
        let tx = tx.clone();
        let state = shared_state.clone();
        let filter = filter.clone();
        let worker_options = std::sync::Arc::clone(&options);
        #[cfg(feature = "gitignore")]
        let gi_cache = Arc::clone(&gi_cache);

        workers.push(thread::spawn(move || {
            loop {
                let mut channel_closed = false;
                let path_item = {
                    let mut st = lock_work_state(&state.0);
                    loop {
                        if let Some(p) = st.queue.pop() {
                            st.active += 1;
                            break Some(p);
                        }
                        if st.active == 0 {
                            state.1.notify_all();
                            break None;
                        }
                        st = wait_for_work(&state.1, st);
                    }
                };

                let Some((path, depth, walk_root, carried_meta)) = path_item else {
                    break;
                };

                if path_exceeds_walk_limit(&path) {
                    let _ = tx.send(WalkItem::Error(WalkError::new(
                        path.clone(),
                        WalkOp::Metadata,
                        std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!(
                                "Fix: shorten path to ≤{} bytes (walkkit MAX_WALK_PATH_BYTES).",
                                crate::walker::MAX_WALK_PATH_BYTES
                            ),
                        ),
                    )));
                    dec_active(&state);
                    continue;
                }

                // Reuse the metadata the parent worker already fetched for this
                // entry during its child scan; only stat here for roots (carried
                // == None), eliminating the redundant second stat per directory.
                let meta = match carried_meta {
                    Some(m) => m,
                    None => match metadata_for_path(&path, worker_options.follow_symlinks) {
                        Ok(m) => m,
                        Err(e) => {
                            if !is_unresolvable_symlink(&path, worker_options.follow_symlinks) {
                                let _ = tx.send(WalkItem::Error(WalkError::new(
                                    path.clone(),
                                    WalkOp::Metadata,
                                    e,
                                )));
                            }
                            dec_active(&state);
                            continue;
                        }
                    },
                };

                #[cfg(feature = "gitignore")]
                let is_ignored = if worker_options.respect_gitignore {
                    match get_cached_gitignore(&gi_cache, &walk_root, &path, meta.is_dir()) {
                        Ok(gi) => path_is_ignored(&gi, &path, meta.is_dir()),
                        Err(e) => {
                            let _ = tx.send(WalkItem::Error(e));
                            dec_active(&state);
                            continue;
                        }
                    }
                } else {
                    false
                };

                #[cfg(not(feature = "gitignore"))]
                let is_ignored = false;

                if is_ignored {
                    dec_active(&state);
                    continue;
                }

                if meta.is_dir() {
                    if worker_options.respect_gitignore
                        && path.file_name().is_some_and(|name| name == ".git")
                    {
                        dec_active(&state);
                        continue;
                    }

                    let dir_key = match dir_id(&path, &meta, worker_options.follow_symlinks) {
                        Ok(k) => k,
                        Err(e) => {
                            let _ = tx.send(WalkItem::Error(WalkError::new(
                                path.clone(),
                                WalkOp::Metadata,
                                e,
                            )));
                            dec_active(&state);
                            continue;
                        }
                    };
                    {
                        let mut st = lock_work_state(&state.0);
                        if !st.visited_dirs.insert(dir_key) {
                            tracing::debug!(path = %path.display(), "skipping directory: cycle detected");
                            signal_done_locked(&mut st, &state.1);
                            continue;
                        }
                    }

                    let recurse = worker_options.max_depth.is_none_or(|max| depth <= max);
                    if recurse {
                        match read_dir_sorted(&path, worker_options.max_dir_entries) {
                            Ok(ReadDirSorted {
                                paths: children,
                                entry_errors,
                            }) => {
                                for e in entry_errors {
                                    let _ = tx.send(WalkItem::Error(WalkError::new(
                                        path.clone(),
                                        WalkOp::ReadDir,
                                        e,
                                    )));
                                }
                                let mut dirs = Vec::new();
                                for child_path in children {
                                    if path_exceeds_walk_limit(&child_path) {
                                        let _ = tx.send(WalkItem::Error(WalkError::new(
                                            child_path.clone(),
                                            WalkOp::Metadata,
                                            std::io::Error::new(
                                                std::io::ErrorKind::InvalidInput,
                                                format!(
                                                    "Fix: shorten path to ≤{} bytes (walkkit MAX_WALK_PATH_BYTES).",
                                                    crate::walker::MAX_WALK_PATH_BYTES
                                                ),
                                            ),
                                        )));
                                        continue;
                                    }
                                    let child_meta = metadata_for_path(
                                        &child_path,
                                        worker_options.follow_symlinks,
                                    );
                                    match child_meta {
                                        Ok(m) => {
                                            if m.is_dir() && recurse {
                                                // Carry the metadata we just
                                                // fetched so the worker that pops
                                                // this dir does not stat it again.
                                                dirs.push((
                                                    child_path,
                                                    depth + 1,
                                                    walk_root.clone(),
                                                    Some(m),
                                                ));
                                            } else if m.is_dir() {
                                                // When recursion is disabled,
                                                // child directories are not walked
                                                // and should not be yielded as files.
                                                // Skip the redundant gitignore and
                                                // build_walked_file work.
                                                continue;
                                            } else {
                                                #[cfg(feature = "gitignore")]
                                                let child_ignored =
                                                    if worker_options.respect_gitignore {
                                                        match get_cached_gitignore(
                                                            &gi_cache,
                                                            &walk_root,
                                                            &child_path,
                                                            false,
                                                        ) {
                                                            Ok(gi) => {
                                                                path_is_ignored(&gi, &child_path, false)
                                                            }
                                                            Err(e) => {
                                                                let _ = tx.send(WalkItem::Error(e));
                                                                channel_closed = true;
                                                                break;
                                                            }
                                                        }
                                                    } else {
                                                        false
                                                    };

                                                #[cfg(not(feature = "gitignore"))]
                                                let child_ignored = false;

                                                if !child_ignored {
                                                    match build_walked_file(
                                                        child_path.clone(),
                                                        &m,
                                                        &filter,
                                                        worker_options
                                                            .extension_filter
                                                            .as_deref(),
                                                        worker_options.size_limit,
                                                        worker_options.skip_binary,
                                                        worker_options.follow_symlinks,
                                                    ) {
                                                        Ok(Some(file)) => {
                                                            let mut st = lock_work_state(&state.0);
                                                            if !st.visited_files.insert(child_path) {
                                                                drop(st);
                                                                continue;
                                                            }
                                                            drop(st);
                                                            if tx.send(WalkItem::File(file)).is_err()
                                                            {
                                                                channel_closed = true;
                                                                break;
                                                            }
                                                        }
                                                        Ok(None) => {}
                                                        Err(e) => {
                                                            let _ = tx.send(WalkItem::Error(e));
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Err(e) => {
                                            if !is_unresolvable_symlink(
                                                &child_path,
                                                worker_options.follow_symlinks,
                                            ) {
                                                let _ = tx.send(WalkItem::Error(WalkError::new(
                                                    child_path,
                                                    WalkOp::Metadata,
                                                    e,
                                                )));
                                            }
                                        }
                                    }
                                    if channel_closed {
                                        break;
                                    }
                                }
                                if !dirs.is_empty() && !channel_closed {
                                    let mut st = lock_work_state(&state.0);
                                    st.queue.extend(dirs);
                                    state.1.notify_all();
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(WalkItem::Error(WalkError::new(
                                    path.clone(),
                                    WalkOp::ReadDir,
                                    e,
                                )));
                            }
                        }
                    }
                } else {
                    match build_walked_file(
                        path.clone(),
                        &meta,
                        &filter,
                        worker_options.extension_filter.as_deref(),
                        worker_options.size_limit,
                        worker_options.skip_binary,
                        worker_options.follow_symlinks,
                    ) {
                        Ok(Some(file)) => {
                            let mut st = lock_work_state(&state.0);
                            if !st.visited_files.insert(path) {
                                signal_done_locked(&mut st, &state.1);
                                continue;
                            }
                            drop(st);
                            if tx.send(WalkItem::File(file)).is_err() {
                                channel_closed = true;
                            }
                        }
                        Ok(None) => {}
                        Err(e) => {
                            let _ = tx.send(WalkItem::Error(e));
                        }
                    }
                }

                let mut st = lock_work_state(&state.0);
                signal_done_locked(&mut st, &state.1);
                if channel_closed {
                    break;
                }
            }
        }));
    }

    for w in workers {
        let _ = w.join();
    }
}
