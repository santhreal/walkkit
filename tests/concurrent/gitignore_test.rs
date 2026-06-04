//! Tests for real .gitignore parsing support.

#![cfg(feature = "gitignore")]
#![allow(clippy::unwrap_used)]

use std::fs;
use tempfile::TempDir;
use walkkit::Walker;

#[test]
fn gitignore_excludes_matching_files() {
    let dir = TempDir::new().unwrap();

    // Create .gitignore
    fs::write(dir.path().join(".gitignore"), "*.log\ntarget/\n").unwrap();

    // Create files
    fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(dir.path().join("debug.log"), "log output").unwrap();
    fs::create_dir(dir.path().join("target")).unwrap();
    fs::write(dir.path().join("target/output.bin"), "binary").unwrap();

    let walker = Walker::new()
        .add_root(dir.path())
        .respect_gitignore(true)
        .with_parallelism(1);

    let files: Vec<String> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .filter_map(|f| f.path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();

    assert!(
        files.contains(&"main.rs".to_string()),
        "main.rs should NOT be ignored"
    );
    assert!(
        !files.contains(&"debug.log".to_string()),
        ".gitignore should exclude *.log files"
    );
    assert!(
        !files.contains(&"output.bin".to_string()),
        ".gitignore should exclude target/ directory contents"
    );
}

#[test]
fn no_gitignore_includes_everything() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(dir.path().join("debug.log"), "log output").unwrap();

    let walker = Walker::new()
        .add_root(dir.path())
        .respect_gitignore(false)
        .with_parallelism(1);

    let files: Vec<String> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .filter_map(|f| f.path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();

    assert!(
        files.contains(&"main.rs".to_string()),
        "main.rs should be included"
    );
    assert!(
        files.contains(&"debug.log".to_string()),
        "without gitignore, debug.log should be included"
    );
}

#[test]
fn nested_gitignore_in_subdirectory() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("subdir");
    fs::create_dir(&sub).unwrap();

    // Root .gitignore ignores *.tmp
    fs::write(dir.path().join(".gitignore"), "*.tmp\n").unwrap();
    // Subdirectory .gitignore ignores *.bak
    fs::write(sub.join(".gitignore"), "*.bak\n").unwrap();

    fs::write(dir.path().join("root.rs"), "fn root() {}").unwrap();
    fs::write(dir.path().join("root.tmp"), "temp file").unwrap();
    fs::write(sub.join("sub.rs"), "fn sub() {}").unwrap();
    fs::write(sub.join("sub.bak"), "backup file").unwrap();
    fs::write(sub.join("sub.tmp"), "also temp").unwrap();

    let walker = Walker::new()
        .add_root(dir.path())
        .respect_gitignore(true)
        .with_parallelism(1);

    let files: Vec<String> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .filter_map(|f| f.path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();

    assert!(
        files.contains(&"root.rs".to_string()),
        "root.rs should be included"
    );
    assert!(
        files.contains(&"sub.rs".to_string()),
        "sub.rs should be included"
    );
    assert!(
        !files.contains(&"root.tmp".to_string()),
        "root.tmp should be ignored by root .gitignore"
    );
    assert!(
        !files.contains(&"sub.bak".to_string()),
        "sub.bak should be ignored by subdir .gitignore"
    );
    assert!(
        !files.contains(&"sub.tmp".to_string()),
        "sub.tmp should be ignored by root .gitignore"
    );
}

#[test]
fn multi_thread_gitignore_excludes_matching_files() {
    let dir = TempDir::new().unwrap();
    let nested = dir.path().join("nested");
    fs::create_dir(&nested).unwrap();

    fs::write(dir.path().join(".gitignore"), "*.log\nignored/\n").unwrap();
    fs::write(dir.path().join("keep.rs"), "fn keep() {}").unwrap();
    fs::write(dir.path().join("skip.log"), "log output").unwrap();
    fs::create_dir(dir.path().join("ignored")).unwrap();
    fs::write(dir.path().join("ignored/file.txt"), "ignored").unwrap();
    fs::write(nested.join(".gitignore"), "*.tmp\n").unwrap();
    fs::write(nested.join("keep.tmp.rs"), "fn nested() {}").unwrap();
    fs::write(nested.join("skip.tmp"), "temp").unwrap();

    let walker = Walker::new()
        .add_root(dir.path())
        .respect_gitignore(true)
        .with_parallelism(4);

    let files: Vec<String> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .map(|f| {
            f.path
                .strip_prefix(dir.path())
                .unwrap()
                .display()
                .to_string()
        })
        .collect();

    assert!(files.contains(&"keep.rs".to_string()));
    assert!(files.contains(&"nested/keep.tmp.rs".to_string()));
    assert!(!files.contains(&"skip.log".to_string()));
    assert!(!files.contains(&"ignored/file.txt".to_string()));
    assert!(!files.contains(&"nested/skip.tmp".to_string()));
}
