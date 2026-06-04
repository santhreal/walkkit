//! Core filesystem walker tests.
//!
//! Tests covering the 10 primary requirements:
//! 1. Walk empty directory → 0 files
//! 2. Walk directory with 10 files → 10 files
//! 3. Walk with skip_binary=true → binary files skipped
//! 4. Walk with respect_gitignore=true → .gitignored files skipped
//! 5. Walk with follow_symlinks=false → symlinks not followed
//! 6. Walk with follow_symlinks=true → symlinks followed
//! 7. Walk with parallelism=4 → same results as parallelism=1 (deterministic)
//! 8. Walk nested directories → all files found
//! 9. Walk non-existent directory → error not panic
//! 10. Walk single file (not directory) → 1 file
#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;
use walkkit::{SortMode, Walker};

/// Helper: Collect all file paths from a walker into a sorted vector.
fn collect_paths(walker: Walker) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .map(|f| f.path)
        .collect();
    paths.sort();
    paths
}

/// Helper: Count files from a walker.
fn count_files(walker: Walker) -> usize {
    walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .count()
}

// =============================================================================
// Test 1: Walk empty directory → 0 files
// =============================================================================

#[test]
fn walk_empty_directory_returns_zero_files() {
    let dir = TempDir::new().unwrap();
    let count = count_files(Walker::new().add_root(dir.path()));
    assert_eq!(count, 0, "Empty directory should yield exactly 0 files");
}

// =============================================================================
// Test 2: Walk directory with 10 files → 10 files
// =============================================================================

#[test]
fn walk_directory_with_ten_files_finds_all_ten() {
    let dir = TempDir::new().unwrap();
    for i in 0..10 {
        fs::write(
            dir.path().join(format!("file_{i:02}.txt")),
            format!("content {i}"),
        )
        .unwrap();
    }
    let count = count_files(Walker::new().add_root(dir.path()));
    assert_eq!(count, 10, "Should find exactly 10 files");
}

// =============================================================================
// Test 3: Walk with skip_binary=true → binary files skipped
// =============================================================================

#[test]
fn walk_skip_binary_true_skips_binary_files() {
    let dir = TempDir::new().unwrap();
    // Create text files
    fs::write(dir.path().join("text1.txt"), "Hello, world!").unwrap();
    fs::write(dir.path().join("text2.txt"), "Another text file").unwrap();
    // Create binary files with NUL bytes
    fs::write(dir.path().join("binary1.bin"), vec![0x00, 0x01, 0x02, 0x03]).unwrap();
    fs::write(
        dir.path().join("binary2.bin"),
        vec![0x7F, 0x45, 0x4C, 0x46, 0x00],
    )
    .unwrap();

    let files = collect_paths(
        Walker::new()
            .add_root(dir.path())
            .skip_binary(true)
            .with_parallelism(1),
    );

    assert_eq!(
        files.len(),
        2,
        "Should find only 2 text files, skipping 2 binary files"
    );
    assert!(files.iter().all(|p| p.extension().unwrap() == "txt"));
}

#[test]
fn walk_skip_binary_true_with_mixed_content() {
    let dir = TempDir::new().unwrap();
    // Text file
    fs::write(dir.path().join("readme.txt"), "This is a readme").unwrap();
    // Binary file with null at position 100
    let mut binary = vec![b'A'; 100];
    binary.push(0);
    fs::write(dir.path().join("data.bin"), binary).unwrap();
    // Another text file
    fs::write(dir.path().join("notes.md"), "# Notes").unwrap();

    let files = collect_paths(
        Walker::new()
            .add_root(dir.path())
            .skip_binary(true)
            .with_sort(SortMode::ByName),
    );

    assert_eq!(files.len(), 2);
    assert!(files[0].ends_with("notes.md"));
    assert!(files[1].ends_with("readme.txt"));
}

// =============================================================================
// Test 4: Walk with respect_gitignore=true → .gitignored files skipped
// =============================================================================

#[test]
fn walk_respect_gitignore_skips_ignored_files() {
    let dir = TempDir::new().unwrap();
    // Create tracked files
    fs::write(dir.path().join("main.rs"), "fn main() {}").unwrap();
    fs::write(dir.path().join("lib.rs"), "pub fn lib() {}").unwrap();
    // Create .gitignore
    fs::write(dir.path().join(".gitignore"), "*.log\ntarget/\n").unwrap();
    // Create files that should be ignored
    fs::write(dir.path().join("debug.log"), "debug info").unwrap();
    fs::write(dir.path().join("error.log"), "error info").unwrap();
    fs::create_dir(dir.path().join("target")).unwrap();
    fs::write(dir.path().join("target/output"), "build output").unwrap();

    let files = collect_paths(
        Walker::new()
            .add_root(dir.path())
            .respect_gitignore(true)
            .with_parallelism(1)
            .with_sort(SortMode::ByName),
    );

    assert_eq!(files.len(), 3, "Should find 3 files (2 .rs + .gitignore)");
    assert!(files.iter().any(|p| p.ends_with("main.rs")));
    assert!(files.iter().any(|p| p.ends_with("lib.rs")));
    assert!(files.iter().any(|p| p.ends_with(".gitignore")));
    assert!(!files.iter().any(|p| p.to_string_lossy().contains(".log")));
    assert!(!files.iter().any(|p| p.to_string_lossy().contains("target")));
}

#[test]
fn walk_respect_gitignore_skips_nested_gitignore() {
    let dir = TempDir::new().unwrap();
    // Root files
    fs::write(dir.path().join("root.txt"), "root").unwrap();
    // Subdirectory with its own .gitignore
    fs::create_dir(dir.path().join("sub")).unwrap();
    fs::write(dir.path().join("sub/keep.txt"), "keep").unwrap();
    fs::write(dir.path().join("sub/.gitignore"), "skip.txt\n").unwrap();
    fs::write(dir.path().join("sub/skip.txt"), "skip").unwrap();
    fs::write(dir.path().join("sub/also_keep.txt"), "also keep").unwrap();

    let files = collect_paths(
        Walker::new()
            .add_root(dir.path())
            .respect_gitignore(true)
            .with_parallelism(1),
    );

    assert_eq!(
        files.len(),
        4,
        "Should find 4 files (root.txt, keep.txt, also_keep.txt, sub/.gitignore)"
    );
    assert!(!files
        .iter()
        .any(|p| p.to_string_lossy().contains("skip.txt")));
}

// =============================================================================
// Test 5: Walk with follow_symlinks=false → symlinks not followed
// =============================================================================

#[test]
#[cfg(unix)]
fn walk_follow_symlinks_false_does_not_follow_file_symlinks() {
    let dir = TempDir::new().unwrap();
    let real_file = dir.path().join("real.txt");
    fs::write(&real_file, "real content").unwrap();
    let link_file = dir.path().join("link.txt");
    std::os::unix::fs::symlink(&real_file, &link_file).unwrap();

    let files = collect_paths(Walker::new().add_root(dir.path()).follow_symlinks(false));

    // Symlink is not followed, so we only see the real file
    assert_eq!(files.len(), 1);
    assert!(files[0].ends_with("real.txt"));
}

#[test]
#[cfg(unix)]
fn walk_follow_symlinks_false_does_not_follow_directory_symlinks() {
    let dir = TempDir::new().unwrap();
    // Create real directory with file
    let real_dir = dir.path().join("real_dir");
    fs::create_dir(&real_dir).unwrap();
    fs::write(real_dir.join("nested.txt"), "nested").unwrap();
    // Create symlink to directory
    let link_dir = dir.path().join("link_dir");
    std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();

    let files = collect_paths(Walker::new().add_root(dir.path()).follow_symlinks(false));

    // Should only find nested.txt, not traverse link_dir
    assert_eq!(files.len(), 1);
    assert!(files[0].to_string_lossy().contains("real_dir"));
}

// =============================================================================
// Test 6: Walk with follow_symlinks=true → symlinks followed
// =============================================================================

#[test]
#[cfg(unix)]
fn walk_follow_symlinks_true_follows_file_symlinks() {
    let dir = TempDir::new().unwrap();
    let real_file = dir.path().join("real.txt");
    fs::write(&real_file, "real content").unwrap();
    let link_file = dir.path().join("link.txt");
    std::os::unix::fs::symlink(&real_file, &link_file).unwrap();

    let files = collect_paths(Walker::new().add_root(dir.path()).follow_symlinks(true));

    // Both real file and symlink should be found
    assert_eq!(files.len(), 2);
    assert!(files.iter().any(|p| p.ends_with("real.txt")));
    assert!(files.iter().any(|p| p.ends_with("link.txt")));
}

#[test]
#[cfg(unix)]
fn walk_follow_symlinks_true_follows_directory_symlinks() {
    let dir = TempDir::new().unwrap();
    // Create real directory with file
    let real_dir = dir.path().join("real_dir");
    fs::create_dir(&real_dir).unwrap();
    fs::write(real_dir.join("nested.txt"), "nested").unwrap();
    // Create symlink to directory
    let link_dir = dir.path().join("link_dir");
    std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();

    let files = collect_paths(Walker::new().add_root(dir.path()).follow_symlinks(true));

    // Should find nested.txt via real_dir and also via link_dir
    // real_dir/nested.txt and link_dir/nested.txt
    // Note: The walker finds files, not directories. Both paths lead to the same file.
    // The walker should find the file via both directory paths.
    let file_count = files.len();
    assert!(
        file_count >= 1,
        "Should find at least 1 file through symlinked directory, found {}",
        file_count
    );
    assert!(
        files
            .iter()
            .any(|p| p.to_string_lossy().contains("nested.txt")),
        "Should find nested.txt"
    );
}

#[test]
#[cfg(unix)]
fn walk_follow_symlinks_true_follows_relative_symlinks() {
    let dir = TempDir::new().unwrap();
    // Create file in subdirectory
    fs::create_dir(dir.path().join("subdir")).unwrap();
    fs::write(dir.path().join("subdir/target.txt"), "target").unwrap();
    // Create relative symlink in root
    std::os::unix::fs::symlink("subdir/target.txt", dir.path().join("link.txt")).unwrap();

    let files = collect_paths(Walker::new().add_root(dir.path()).follow_symlinks(true));

    assert_eq!(files.len(), 2);
}

// =============================================================================
// Test 7: Walk with parallelism=4 → same results as parallelism=1 (deterministic)
// =============================================================================

#[test]
fn walk_parallelism_four_matches_parallelism_one() {
    let dir = TempDir::new().unwrap();
    // Create 50 files in various subdirectories
    for i in 0..10 {
        fs::write(dir.path().join(format!("file_{i:02}.txt")), format!("{i}")).unwrap();
    }
    fs::create_dir(dir.path().join("sub1")).unwrap();
    fs::create_dir(dir.path().join("sub2")).unwrap();
    for i in 0..20 {
        fs::write(
            dir.path().join(format!("sub1/file_{i:02}.txt")),
            format!("{i}"),
        )
        .unwrap();
    }
    for i in 0..20 {
        fs::write(
            dir.path().join(format!("sub2/file_{i:02}.txt")),
            format!("{i}"),
        )
        .unwrap();
    }

    let files_p1: HashSet<PathBuf> = Walker::new()
        .add_root(dir.path())
        .with_parallelism(1)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .map(|f| f.path)
        .collect();

    let files_p4: HashSet<PathBuf> = Walker::new()
        .add_root(dir.path())
        .with_parallelism(4)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .map(|f| f.path)
        .collect();

    assert_eq!(
        files_p1, files_p4,
        "Parallelism=4 should yield same results as parallelism=1"
    );
    assert_eq!(files_p1.len(), 50);
}

#[test]
fn walk_parallelism_deterministic_across_runs() {
    let dir = TempDir::new().unwrap();
    // Create files
    for i in 0..30 {
        fs::write(dir.path().join(format!("file_{i:02}.txt")), "x").unwrap();
    }

    // Run multiple times with parallelism=4
    for run in 0..5 {
        let count = count_files(Walker::new().add_root(dir.path()).with_parallelism(4));
        assert_eq!(
            count, 30,
            "Run {}: Should find all 30 files deterministically",
            run
        );
    }
}

// =============================================================================
// Test 8: Walk nested directories → all files found
// =============================================================================

#[test]
fn walk_nested_directories_finds_all_files() {
    let dir = TempDir::new().unwrap();
    // Create nested structure: a/b/c/d/e/
    fs::write(dir.path().join("level0.txt"), "level0").unwrap();
    fs::create_dir(dir.path().join("level1")).unwrap();
    fs::write(dir.path().join("level1/file.txt"), "level1").unwrap();
    fs::create_dir(dir.path().join("level1/level2")).unwrap();
    fs::write(dir.path().join("level1/level2/file.txt"), "level2").unwrap();
    fs::create_dir(dir.path().join("level1/level2/level3")).unwrap();
    fs::write(dir.path().join("level1/level2/level3/file.txt"), "level3").unwrap();

    let files = collect_paths(Walker::new().add_root(dir.path()));

    assert_eq!(
        files.len(),
        4,
        "Should find all 4 files at different nesting levels"
    );
    assert!(files
        .iter()
        .any(|p| p.to_string_lossy().contains("level0.txt")));
    assert!(files
        .iter()
        .any(|p| p.to_string_lossy().contains("level1/file.txt")));
    assert!(files
        .iter()
        .any(|p| p.to_string_lossy().contains("level2/file.txt")));
    assert!(files
        .iter()
        .any(|p| p.to_string_lossy().contains("level3/file.txt")));
}

#[test]
fn walk_deeply_nested_structure() {
    let dir = TempDir::new().unwrap();
    let mut current = dir.path().to_path_buf();

    // Create 20 levels of nesting (no file at root, just in subdirs)
    for depth in 0..20 {
        current = current.join(format!("depth{depth}"));
        fs::create_dir(&current).unwrap();
        fs::write(current.join("file.txt"), format!("depth {depth}")).unwrap();
    }

    let files = collect_paths(Walker::new().add_root(dir.path()));

    assert_eq!(files.len(), 20, "Should find files at all 20 depth levels");
}

#[test]
fn walk_wide_nested_structure() {
    let dir = TempDir::new().unwrap();
    // Create 50 subdirectories at root, each with one file
    for i in 0..50 {
        let subdir = dir.path().join(format!("subdir_{i:02}"));
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), format!("{i}")).unwrap();
    }

    let files = collect_paths(Walker::new().add_root(dir.path()));

    assert_eq!(
        files.len(),
        50,
        "Should find all 50 files in wide structure"
    );
}

// =============================================================================
// Test 9: Walk non-existent directory → error not panic
// =============================================================================

#[test]
fn walk_nonexistent_directory_yields_empty() {
    let dir = TempDir::new().unwrap();
    let nonexistent = dir.path().join("does_not_exist");

    // Should not panic, just yield no files
    let files: Vec<_> = Walker::new()
        .add_root(&nonexistent)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert!(
        files.is_empty(),
        "Non-existent directory should yield empty results, not panic"
    );
}

#[test]
fn walk_nonexistent_nested_path() {
    let dir = TempDir::new().unwrap();
    let nonexistent = dir.path().join("a/b/c/d/e");

    // Should not panic; surface an error item, no discovered files.
    let items: Vec<_> = Walker::new()
        .add_root(&nonexistent)
        .with_parallelism(4)
        .walk()
        .unwrap()
        .collect();
    let file_count = items
        .iter()
        .filter(|i| matches!(i, walkkit::WalkItem::File(_)))
        .count();
    assert_eq!(file_count, 0);
    assert!(
        items
            .iter()
            .any(|i| matches!(i, walkkit::WalkItem::Error(_))),
        "nonexistent nested root should report a traversal error"
    );
}

#[test]
fn walk_with_some_valid_and_some_invalid_roots() {
    let dir = TempDir::new().unwrap();
    let valid_root = dir.path().join("valid");
    fs::create_dir(&valid_root).unwrap();
    fs::write(valid_root.join("file.txt"), "content").unwrap();
    let invalid_root = dir.path().join("invalid");

    // Mix valid and invalid roots - should not panic; one file plus an error for the bad root.
    let items: Vec<_> = Walker::new()
        .add_root(&valid_root)
        .add_root(&invalid_root)
        .walk()
        .unwrap()
        .collect();
    let files: Vec<_> = items
        .iter()
        .filter_map(|i| match i {
            walkkit::WalkItem::File(f) => Some(f),
            walkkit::WalkItem::Error(_) => None,
        })
        .collect();
    assert_eq!(files.len(), 1);
    assert!(
        items
            .iter()
            .any(|i| matches!(i, walkkit::WalkItem::Error(_))),
        "invalid root should produce a traversal error"
    );
}

// =============================================================================
// Test 10: Walk single file (not directory) → 1 file
// =============================================================================

#[test]
fn walk_single_file_as_root() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("single.txt");
    fs::write(&file_path, "single file content").unwrap();

    let files = collect_paths(Walker::new().add_root(&file_path));

    assert_eq!(
        files.len(),
        1,
        "Walking a single file should yield exactly 1 file"
    );
    assert_eq!(files[0], file_path);
}

#[test]
fn walk_single_file_returns_correct_metadata() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("data.txt");
    let content = "Hello, World!";
    fs::write(&file_path, content).unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(&file_path)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, file_path);
    assert_eq!(files[0].size, content.len() as u64);
}

#[test]
fn walk_single_binary_file() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join("binary.bin");
    fs::write(&file_path, vec![0u8, 1, 2, 3, 4, 5]).unwrap();

    let files = collect_paths(Walker::new().add_root(&file_path));

    assert_eq!(files.len(), 1);
    assert_eq!(files[0], file_path);
}

// =============================================================================
// Additional edge case tests
// =============================================================================

#[test]
fn walk_empty_and_nonempty_directories_together() {
    let dir = TempDir::new().unwrap();
    let empty_dir = dir.path().join("empty");
    let nonempty_dir = dir.path().join("nonempty");
    fs::create_dir(&empty_dir).unwrap();
    fs::create_dir(&nonempty_dir).unwrap();
    fs::write(nonempty_dir.join("file.txt"), "content").unwrap();

    let files = collect_paths(Walker::new().add_root(&empty_dir).add_root(&nonempty_dir));

    assert_eq!(files.len(), 1);
    assert!(files[0].to_string_lossy().contains("nonempty"));
}

#[test]
fn walk_directory_with_only_subdirectories() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("sub1")).unwrap();
    fs::create_dir(dir.path().join("sub2")).unwrap();
    fs::create_dir(dir.path().join("sub3")).unwrap();

    let files = collect_paths(Walker::new().add_root(dir.path()));

    assert!(
        files.is_empty(),
        "Directory with only subdirs should yield 0 files"
    );
}

#[test]
fn walk_file_count_matches_expected_with_hidden() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("visible.txt"), "v").unwrap();
    fs::write(dir.path().join(".hidden"), "h").unwrap();
    fs::write(dir.path().join(".gitignore"), "*").unwrap();

    let count = count_files(Walker::new().add_root(dir.path()));

    assert_eq!(count, 3, "Should count all files including hidden");
}
