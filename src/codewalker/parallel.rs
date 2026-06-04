use std::sync::mpsc;
use std::sync::{Arc, Mutex};

use ignore::overrides::OverrideBuilder;
use ignore::WalkBuilder;

use super::{CodeWalker, FileEntry, WalkConfig};
use crate::filter::{code_entry_allowed, code_process_path};

pub(crate) const PARALLEL_CHANNEL_CAPACITY: usize = 8_192;

impl CodeWalker {
    /// Walk the tree in parallel, sending entries to a channel.
    ///
    /// Returns a receiver. The walker runs on `threads` background threads.
    /// Entries arrive as they're discovered  -  no need to wait for the full walk.
    #[must_use]
    pub fn walk_parallel(&self, threads: usize) -> mpsc::Receiver<crate::error::Result<FileEntry>> {
        let (tx, rx) = mpsc::sync_channel(PARALLEL_CHANNEL_CAPACITY);
        let config = Arc::new(self.config.clone());
        let root = self.root.clone();
        let error_slot: Arc<Mutex<Option<crate::error::CodewalkError>>> =
            Arc::new(Mutex::new(None));
        let walker =
            self.build_parallel_walker(threads, Arc::clone(&config), root, Arc::clone(&error_slot));

        std::thread::spawn(move || {
            walker.run(|| {
                let tx = tx.clone();
                let config = Arc::clone(&config);
                let error_slot = Arc::clone(&error_slot);
                Box::new(move |result| {
                    // Propagate any error stashed by filter_entry.
                    if let Some(err) = error_slot
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .take()
                    {
                        if tx.send(Err(err)).is_err() {
                            return ignore::WalkState::Quit;
                        }
                    }
                    let entry_result = match result {
                        Ok(entry) => match entry.file_type() {
                            Some(ft) if ft.is_file() => {
                                match code_process_path(entry.path(), config.as_ref()) {
                                    Ok(Some(file_entry)) => Ok(file_entry),
                                    Ok(None) => return ignore::WalkState::Continue,
                                    Err(err) => Err(err),
                                }
                            }
                            _ => return ignore::WalkState::Continue,
                        },
                        Err(err) => Err(crate::error::CodewalkError::Ignore(err)),
                    };

                    if tx.send(entry_result).is_err() {
                        return ignore::WalkState::Quit;
                    }

                    ignore::WalkState::Continue
                })
            });
            // After all workers finish, check for a final stashed error.
            if let Some(err) = Arc::try_unwrap(error_slot)
                .ok()
                .and_then(|m| m.into_inner().ok())
                .flatten()
            {
                let _ = tx.send(Err(err));
            }
        });

        rx
    }

    pub(crate) fn build_parallel_walker(
        &self,
        threads: usize,
        config: Arc<WalkConfig>,
        root: std::path::PathBuf,
        error_slot: Arc<Mutex<Option<crate::error::CodewalkError>>>,
    ) -> ignore::WalkParallel {
        let mut builder = WalkBuilder::new(&self.root);
        builder
            .hidden(self.config.skip_hidden)
            .git_ignore(self.config.respect_gitignore)
            .git_global(self.config.respect_gitignore)
            .git_exclude(self.config.respect_gitignore)
            .follow_links(self.config.follow_symlinks)
            .threads(threads);

        for ignore_file in &config.ignore_files {
            builder.add_custom_ignore_filename(ignore_file);
        }

        if !config.ignore_patterns.is_empty() {
            let mut ovr = OverrideBuilder::new(&self.root);
            for pattern in &config.ignore_patterns {
                if let Err(err) = ovr.add(pattern) {
                    tracing::warn!(pattern = %pattern, error = %err, "invalid ignore pattern");
                }
            }
            match ovr.build() {
                Ok(overrides) => {
                    builder.overrides(overrides);
                }
                Err(err) => {
                    tracing::warn!(error = %err, "failed to build ignore overrides");
                }
            }
        }

        let exclude_dirs = self.config.exclude_dirs.clone();
        builder
            .filter_entry(move |entry| {
                if entry.file_type().is_some_and(|ft| ft.is_dir()) {
                    let name = entry.file_name().to_string_lossy();
                    if exclude_dirs.contains(name.as_ref()) {
                        return false;
                    }
                }
                match code_entry_allowed(entry.path(), &root, config.as_ref()) {
                    Ok(allowed) => allowed,
                    Err(e) => {
                        *error_slot
                            .lock()
                            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(e);
                        false
                    }
                }
            })
            .build_parallel()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::codewalker::test_utils::setup_test_dir;

    #[test]
    fn parallel_walk() {
        let dir = setup_test_dir();
        let walker = CodeWalker::new(dir.path(), WalkConfig::default());
        let rx = walker.walk_parallel(2);
        let entries: Vec<FileEntry> = rx.iter().collect::<Result<Vec<_>, _>>().unwrap();
        assert!(entries.len() >= 2);
    }
}
