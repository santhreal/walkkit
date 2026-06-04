//! Security Audit Tests for walkkit
//!
//! These tests verify critical security properties:
//! 1. Symlink following: no infinite loops on circular symlinks
//! 2. Permission denied: skip file and continue, not abort scan
//! 3. Large directories: must not OOM
//! 4. Hidden files: must be discoverable
//! 5. Binary file detection: no false negatives
//!
//! Every finding is CRITICAL at internet scale.
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

// =============================================================================
// Helper Functions
// =============================================================================

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

fn count_files(walker: Walker) -> usize {
    walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .count()
}

// =============================================================================
// TEST 1: Empty Directory
// =============================================================================

#[test]
fn audit_empty_directory_yields_zero_files() {
    let dir = TempDir::new().unwrap();
    let count = count_files(Walker::new().add_root(dir.path()));
    assert_eq!(count, 0, "Fix: Empty directory must yield exactly 0 files");
}

#[test]
fn audit_empty_directory_with_parallelism() {
    let dir = TempDir::new().unwrap();
    for threads in [1, 2, 4, 8] {
        let count = count_files(Walker::new().add_root(dir.path()).with_parallelism(threads));
        assert_eq!(
            count, 0,
            "Fix: Empty directory must yield 0 files with parallelism={}",
            threads
        );
    }
}

// =============================================================================
// TEST 2: Deeply Nested Directories
// =============================================================================

#[test]
fn audit_deeply_nested_directories_100_levels() {
    let dir = TempDir::new().unwrap();
    let mut current = dir.path().to_path_buf();

    // Create 100 levels of nesting
    for depth in 0..100 {
        current = current.join(format!("depth{depth:03}"));
        fs::create_dir(&current).unwrap();
        fs::write(current.join("marker.txt"), format!("depth {depth}")).unwrap();
    }

    let files = collect_paths(Walker::new().add_root(dir.path()));

    assert_eq!(
        files.len(),
        100,
        "Fix: Must find all 100 files at different depth levels"
    );

    // Verify all depth markers are present (don't rely on order)
    let contents: std::collections::HashSet<String> = files
        .iter()
        .map(|f| fs::read_to_string(f).unwrap())
        .collect();

    for depth in 0..100 {
        let expected = format!("depth {depth}");
        assert!(
            contents.contains(&expected),
            "Fix: Missing depth marker for depth {}",
            depth
        );
    }
}

#[test]
fn audit_deeply_nested_directories_500_levels() {
    let dir = TempDir::new().unwrap();
    let mut current = dir.path().to_path_buf();

    // Create 500 levels of nesting
    for depth in 0..500 {
        current = current.join(format!("d{depth:04}"));
        let _ = fs::create_dir(&current);
    }

    // Write file at deepest level
    let _ = fs::write(current.join("deep.txt"), "found");

    let files = collect_paths(Walker::new().add_root(dir.path()));

    // Should find the deep file if OS supports that depth
    if current.join("deep.txt").exists() {
        assert!(
            files.iter().any(|p| p.ends_with("deep.txt")),
            "Fix: Must find file at 500 levels deep"
        );
    }
}

#[test]
fn audit_deep_nesting_with_early_exit() {
    let dir = TempDir::new().unwrap();
    let mut current = dir.path().to_path_buf();

    // Create deep nesting with files at multiple levels
    for depth in 0..50 {
        current = current.join(format!("level{depth}"));
        fs::create_dir(&current).unwrap();
        if depth % 10 == 0 {
            fs::write(current.join("marker.txt"), format!("level{depth}")).unwrap();
        }
    }

    let files = collect_paths(Walker::new().add_root(dir.path()));

    assert_eq!(
        files.len(),
        5,
        "Fix: Must find all 5 marker files at depths 0, 10, 20, 30, 40"
    );
}

// =============================================================================
// TEST 3: Symlink Loops
// =============================================================================

#[test]
#[cfg(unix)]
fn audit_symlink_self_loop_detection() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(root.join("real.txt"), "content").unwrap();
    fs::create_dir(root.join("sub")).unwrap();

    // Create a -> a symlink (self-loop)
    std::os::unix::fs::symlink(root, root.join("sub/loop")).unwrap();

    let start = std::time::Instant::now();
    let files = collect_paths(
        Walker::new()
            .add_root(root)
            .follow_symlinks(true)
            .with_parallelism(1),
    );
    let elapsed = start.elapsed();

    // Must complete quickly (not infinite loop)
    assert!(
        elapsed.as_secs() < 5,
        "Fix: Self-loop caused timeout - infinite loop detected"
    );
    assert_eq!(
        files.len(),
        1,
        "Fix: Must find exactly 1 file, not loop infinitely"
    );
}

#[test]
#[cfg(unix)]
fn audit_symlink_mutual_loop_detection() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let dir_a = root.join("a");
    let dir_b = root.join("b");
    fs::create_dir(&dir_a).unwrap();
    fs::create_dir(&dir_b).unwrap();

    fs::write(dir_a.join("file_a.txt"), "a").unwrap();
    fs::write(dir_b.join("file_b.txt"), "b").unwrap();

    // Create mutual loop: a/link_to_b -> b, b/link_to_a -> a
    std::os::unix::fs::symlink(&dir_b, dir_a.join("link_to_b")).unwrap();
    std::os::unix::fs::symlink(&dir_a, dir_b.join("link_to_a")).unwrap();

    let start = std::time::Instant::now();
    let files = collect_paths(
        Walker::new()
            .add_root(root)
            .follow_symlinks(true)
            .with_parallelism(1),
    );
    let elapsed = start.elapsed();

    // Must complete quickly
    assert!(elapsed.as_secs() < 5, "Fix: Mutual loop caused timeout");

    // Should find the original files, possibly via symlink paths
    assert!(files.len() >= 2, "Fix: Must find at least 2 files");
}

#[test]
#[cfg(unix)]
fn audit_symlink_triangle_loop_detection() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let dir_a = root.join("a");
    let dir_b = root.join("b");
    let dir_c = root.join("c");
    fs::create_dir(&dir_a).unwrap();
    fs::create_dir(&dir_b).unwrap();
    fs::create_dir(&dir_c).unwrap();

    fs::write(dir_a.join("file.txt"), "test").unwrap();

    // Create triangle loop: a->b, b->c, c->a
    std::os::unix::fs::symlink(&dir_b, dir_a.join("to_b")).unwrap();
    std::os::unix::fs::symlink(&dir_c, dir_b.join("to_c")).unwrap();
    std::os::unix::fs::symlink(&dir_a, dir_c.join("to_a")).unwrap();

    let start = std::time::Instant::now();
    let files = collect_paths(
        Walker::new()
            .add_root(root)
            .follow_symlinks(true)
            .with_parallelism(1),
    );
    let elapsed = start.elapsed();

    assert!(elapsed.as_secs() < 5, "Fix: Triangle loop caused timeout");
    assert!(files.len() >= 1, "Fix: Must find at least 1 file");
}

#[test]
#[cfg(unix)]
fn audit_symlink_chain_loop_detection() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create chain of symlinks that eventually loops back
    fs::create_dir(root.join("dir")).unwrap();
    fs::write(root.join("dir/file.txt"), "content").unwrap();

    // link1 -> dir
    std::os::unix::fs::symlink(root.join("dir"), root.join("link1")).unwrap();
    // link2 -> link1
    std::os::unix::fs::symlink(root.join("link1"), root.join("link2")).unwrap();
    // link3 -> link2
    std::os::unix::fs::symlink(root.join("link2"), root.join("link3")).unwrap();

    let start = std::time::Instant::now();
    let files = collect_paths(
        Walker::new()
            .add_root(root)
            .follow_symlinks(true)
            .with_parallelism(1),
    );
    let elapsed = start.elapsed();

    assert!(elapsed.as_secs() < 5, "Fix: Chain loop caused timeout");
    assert!(files.len() >= 1, "Fix: Must find the original file");
}

#[test]
#[cfg(unix)]
fn audit_symlink_parallel_multi_thread_loop_detection() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create multiple directories with symlink loops
    for i in 0..5 {
        let subdir = root.join(format!("dir{}", i));
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), format!("{}", i)).unwrap();
        // Each directory has a loop back to itself
        std::os::unix::fs::symlink(&subdir, subdir.join("loop")).unwrap();
    }

    let start = std::time::Instant::now();
    let files = collect_paths(
        Walker::new()
            .add_root(root)
            .follow_symlinks(true)
            .with_parallelism(4),
    );
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_secs() < 5,
        "Fix: Multi-threaded loop detection failed"
    );
    assert_eq!(
        files.len(),
        5,
        "Fix: Must find exactly 5 files (one per dir)"
    );
}

// =============================================================================
// TEST 4: Permission Denied Handling
// =============================================================================

#[test]
#[cfg(unix)]
fn audit_permission_denied_directory_is_skipped() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create accessible file
    fs::write(root.join("accessible.txt"), "yes").unwrap();

    // Create restricted subdirectory
    let restricted = root.join("restricted");
    fs::create_dir(&restricted).unwrap();
    fs::write(restricted.join("secret.txt"), "no").unwrap();

    // Remove read permission
    let mut perms = fs::metadata(&restricted).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&restricted, perms).unwrap();

    let files = collect_paths(Walker::new().add_root(root));

    // Restore permissions for cleanup
    let mut perms = fs::metadata(&restricted).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&restricted, perms).unwrap();

    // Must find accessible file, skip restricted without panic
    assert_eq!(
        files.len(),
        1,
        "Fix: Must find 1 accessible file, skip restricted dir"
    );
    assert!(
        files[0].ends_with("accessible.txt"),
        "Fix: Must find accessible.txt"
    );
}

#[test]
#[cfg(unix)]
fn audit_permission_denied_with_parallelism() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create multiple subdirectories, one restricted
    for i in 0..5 {
        let subdir = root.join(format!("dir{}", i));
        fs::create_dir(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), format!("{}", i)).unwrap();
    }

    // Restrict one directory
    let restricted = root.join("dir2");
    let mut perms = fs::metadata(&restricted).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&restricted, perms).unwrap();

    let files = collect_paths(Walker::new().add_root(root).with_parallelism(4));

    // Restore permissions
    let mut perms = fs::metadata(&restricted).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&restricted, perms).unwrap();

    // Must find 4 files (all except restricted)
    assert_eq!(
        files.len(),
        4,
        "Fix: Must find 4 files, skip restricted dir2"
    );
}

#[test]
#[cfg(unix)]
fn audit_permission_denied_root_directory() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let root = dir.path().join("unreadable_root");
    fs::create_dir(&root).unwrap();
    fs::write(root.join("file.txt"), "test").unwrap();

    // Make root unreadable
    let mut perms = fs::metadata(&root).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&root, perms).unwrap();

    let files = collect_paths(Walker::new().add_root(&root));

    // Restore permissions
    let mut perms = fs::metadata(&root).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&root, perms).unwrap();

    // Should yield no files but not panic
    assert!(
        files.is_empty(),
        "Fix: Unreadable root should yield 0 files without panic"
    );
}

// =============================================================================
// TEST 5: Hidden Files (.dot files)
// =============================================================================

#[test]
fn audit_hidden_files_discoverable() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(root.join("visible.txt"), "visible").unwrap();
    fs::write(root.join(".hidden"), "hidden").unwrap();
    fs::write(root.join(".gitignore"), "gitignore").unwrap();
    fs::write(root.join(".env"), "secret").unwrap();

    let files: HashSet<_> = Walker::new()
        .add_root(root)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .filter_map(|f| f.path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();

    assert!(files.contains("visible.txt"), "Fix: Must find visible.txt");
    assert!(files.contains(".hidden"), "Fix: Must find .hidden");
    assert!(files.contains(".gitignore"), "Fix: Must find .gitignore");
    assert!(files.contains(".env"), "Fix: Must find .env");
    assert_eq!(
        files.len(),
        4,
        "Fix: Must find all 4 files including hidden"
    );
}

#[test]
fn audit_hidden_directories_discoverable() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create visible and hidden directories
    fs::create_dir(root.join("visible_dir")).unwrap();
    fs::create_dir(root.join(".hidden_dir")).unwrap();

    fs::write(root.join("visible_dir/file.txt"), "v").unwrap();
    fs::write(root.join(".hidden_dir/file.txt"), "h").unwrap();

    let files = collect_paths(Walker::new().add_root(root));

    assert_eq!(
        files.len(),
        2,
        "Fix: Must find both files in visible and hidden dirs"
    );
}

#[test]
fn audit_hidden_files_with_filter() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(root.join("config.txt"), "config").unwrap();
    fs::write(root.join(".config"), "hidden_config").unwrap();
    fs::write(root.join(".hidden.txt"), "hidden_text").unwrap();

    let files: HashSet<_> = Walker::new()
        .add_root(root)
        .with_extension_filter("txt")
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .filter_map(|f| f.path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();

    assert!(files.contains("config.txt"), "Fix: Must find config.txt");
    assert!(files.contains(".hidden.txt"), "Fix: Must find .hidden.txt");
    assert!(
        !files.contains(".config"),
        "Fix: .config has no extension, should not match"
    );
}

// =============================================================================
// TEST 6: Binary File Detection
// =============================================================================

#[test]
fn audit_binary_detection_null_byte() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Text file
    fs::write(root.join("text.txt"), "Hello, World!").unwrap();
    // Binary file with null byte
    fs::write(root.join("binary.bin"), vec![0x7F, b'E', b'L', b'F', 0x00]).unwrap();

    let files = collect_paths(Walker::new().add_root(root).skip_binary(true));

    assert_eq!(files.len(), 1, "Fix: Must skip binary file with null byte");
    assert!(
        files[0].ends_with("text.txt"),
        "Fix: Must only return text file"
    );
}

#[test]
fn audit_binary_detection_null_at_8kb_boundary() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Text file
    fs::write(root.join("text.txt"), "text").unwrap();

    // Binary with null at position 8191 (within 8KB)
    let mut content = vec![b'A'; 8191];
    content.push(0);
    fs::write(root.join("binary_at_8k.bin"), &content).unwrap();

    let files = collect_paths(Walker::new().add_root(root).skip_binary(true));

    assert_eq!(files.len(), 1, "Fix: Must detect null at 8KB boundary");
}

#[test]
fn audit_binary_detection_null_after_8kb_is_text() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Content with null at position 8192. The improved binary detection reads
    // the full file for files ≤ 10MB, so NUL bytes anywhere are detected.
    let mut content = vec![b'A'; 8192];
    content.push(0);
    fs::write(root.join("after_8k.bin"), &content).unwrap();

    let files = collect_paths(Walker::new().add_root(root).skip_binary(true));

    // Full-file binary detection correctly identifies NUL at position 8192
    assert_eq!(
        files.len(),
        0,
        "NUL at any position in files ≤ 10MB is detected"
    );
}

#[test]
fn audit_binary_detection_elf_header() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(root.join("script.sh"), "#!/bin/bash\necho hello").unwrap();
    // ELF header: 0x7F 'E' 'L' 'F' followed by null
    fs::write(
        root.join("program"),
        vec![0x7F, 0x45, 0x4C, 0x46, 0x01, 0x00],
    )
    .unwrap();

    let files = collect_paths(Walker::new().add_root(root).skip_binary(true));

    assert_eq!(files.len(), 1, "Fix: Must skip ELF binary");
    assert!(
        files[0].ends_with("script.sh"),
        "Fix: Must only return script"
    );
}

#[test]
fn audit_binary_detection_mixed_directory() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create mix of text and binary files
    for i in 0..10 {
        fs::write(
            root.join(format!("text_{}.txt", i)),
            format!("content {}", i),
        )
        .unwrap();
    }
    for i in 0..10 {
        fs::write(
            root.join(format!("binary_{}.bin", i)),
            vec![0u8, i as u8, 0xFF, 0xFE],
        )
        .unwrap();
    }

    let files = collect_paths(Walker::new().add_root(root).skip_binary(true));

    assert_eq!(files.len(), 10, "Fix: Must find exactly 10 text files");
    assert!(
        files.iter().all(|p| p.extension().unwrap() == "txt"),
        "Fix: All returned files should be .txt"
    );
}

// =============================================================================
// TEST 7: Max File Count Stress Test
// =============================================================================

#[test]
fn audit_max_file_count_1000_files() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    for i in 0..1000 {
        fs::write(
            root.join(format!("file_{:04}.txt", i)),
            format!("content {}", i),
        )
        .unwrap();
    }

    let files = collect_paths(Walker::new().add_root(root));

    assert_eq!(files.len(), 1000, "Fix: Must find all 1000 files");
}

#[test]
fn audit_max_file_count_10000_files() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    for i in 0..10000 {
        fs::write(root.join(format!("file_{:05}.txt", i)), "x").unwrap();
    }

    let count = count_files(Walker::new().add_root(root));

    assert_eq!(count, 10000, "Fix: Must find all 10000 files without OOM");
}

#[test]
fn audit_max_file_count_with_parallelism() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    for i in 0..5000 {
        fs::write(root.join(format!("file_{}.txt", i)), "x").unwrap();
    }

    let count = count_files(Walker::new().add_root(root).with_parallelism(8));

    assert_eq!(
        count, 5000,
        "Fix: Must find all 5000 files with parallelism=8"
    );
}

#[test]
fn audit_wide_directory_structure() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create 100 subdirectories, each with 100 files = 10,000 files
    for d in 0..100 {
        let subdir = root.join(format!("dir{:03}", d));
        fs::create_dir(&subdir).unwrap();
        for f in 0..100 {
            fs::write(subdir.join(format!("file{:03}.txt", f)), "x").unwrap();
        }
    }

    let count = count_files(Walker::new().add_root(root).with_parallelism(4));

    assert_eq!(
        count, 10000,
        "Fix: Must find all 10,000 files in wide structure"
    );
}

// =============================================================================
// TEST 8: Combined Adversarial Scenarios
// =============================================================================

#[test]
#[cfg(unix)]
fn audit_combined_symlinks_and_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create structure: visible/accessible, hidden/restricted, symlink loop
    let visible = root.join("visible");
    let restricted = root.join("restricted");
    fs::create_dir(&visible).unwrap();
    fs::create_dir(&restricted).unwrap();

    fs::write(visible.join("file.txt"), "v").unwrap();
    fs::write(restricted.join("secret.txt"), "s").unwrap();

    // Restrict one directory
    let mut perms = fs::metadata(&restricted).unwrap().permissions();
    perms.set_mode(0o000);
    fs::set_permissions(&restricted, perms).unwrap();

    // Create symlink loop
    std::os::unix::fs::symlink(&visible, visible.join("loop")).unwrap();

    let start = std::time::Instant::now();
    let files = collect_paths(Walker::new().add_root(root).follow_symlinks(true));
    let elapsed = start.elapsed();

    // Restore permissions
    let mut perms = fs::metadata(&restricted).unwrap().permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&restricted, perms).unwrap();

    assert!(
        elapsed.as_secs() < 5,
        "Fix: Combined adversarial scenario caused timeout"
    );
    assert_eq!(files.len(), 1, "Fix: Must find only visible/file.txt");
}

#[test]
fn audit_combined_hidden_binary_and_large() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Mix of hidden, binary, text files
    for i in 0..100 {
        // Visible text
        fs::write(root.join(format!("text_{}.txt", i)), "text").unwrap();
        // Hidden text
        fs::write(root.join(format!(".hidden_{}", i)), "hidden").unwrap();
        // Binary
        fs::write(root.join(format!("binary_{}.bin", i)), vec![0u8, i as u8]).unwrap();
    }

    let files: HashSet<_> = Walker::new()
        .add_root(root)
        .skip_binary(true)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .filter_map(|f| f.path.file_name().map(|n| n.to_string_lossy().into_owned()))
        .collect();

    // Should find all text files (visible and hidden), skip binary
    assert_eq!(
        files.len(),
        200,
        "Fix: Must find 100 visible + 100 hidden text files"
    );

    for i in 0..100 {
        assert!(
            files.contains(&format!("text_{}.txt", i)),
            "Fix: Missing text file {}",
            i
        );
        assert!(
            files.contains(&format!(".hidden_{}", i)),
            "Fix: Missing hidden file {}",
            i
        );
    }
}

#[test]
fn audit_duplicate_root_file_deduplicated_single_threaded() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let file_path = root.join("single.txt");
    fs::write(&file_path, "content").unwrap();

    let files = collect_paths(
        Walker::new()
            .add_root(&file_path)
            .add_root(&file_path)
            .with_parallelism(1),
    );

    assert_eq!(
        files.len(),
        1,
        "Fix: Duplicate file root must be deduplicated in single-threaded walk"
    );
    assert_eq!(files[0], file_path);
}

#[test]
fn audit_duplicate_root_file_deduplicated_multi_threaded() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    let file_path = root.join("single.txt");
    fs::write(&file_path, "content").unwrap();

    let files = collect_paths(
        Walker::new()
            .add_root(&file_path)
            .add_root(&file_path)
            .with_parallelism(4),
    );

    assert_eq!(
        files.len(),
        1,
        "Fix: Duplicate file root must be deduplicated in multi-threaded walk"
    );
    assert_eq!(files[0], file_path);
}

#[test]
fn audit_parallelism_capped_prevents_oom() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("file.txt"), "x").unwrap();

    // Before the fix, usize::MAX could cause OOM or crash.
    // After the fix, it is clamped to a safe maximum.
    let files = collect_paths(
        Walker::new()
            .add_root(root)
            .with_parallelism(usize::MAX)
            .with_sort(SortMode::ByName),
    );

    assert_eq!(
        files.len(),
        1,
        "Fix: with_parallelism must be capped to prevent OOM/crash"
    );
}

#[test]
fn audit_filter_rejects_empty_and_null_patterns() {
    use walkkit::FileFilter;

    let empty_filter = FileFilter::new().add_include("");
    assert!(
        empty_filter.compile().is_err(),
        "Fix: empty glob pattern must be rejected"
    );

    let null_filter = FileFilter::new().add_include("foo\0bar");
    assert!(
        null_filter.compile().is_err(),
        "Fix: null-byte glob pattern must be rejected"
    );
}

#[test]
fn audit_deep_nesting_with_hidden_and_binary() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    let mut current = root.to_path_buf();
    for depth in 0..50 {
        current = current.join(format!("level{}", depth));
        fs::create_dir(&current).unwrap();

        if depth % 5 == 0 {
            // Visible text at this level
            fs::write(current.join("visible.txt"), "v").unwrap();
            // Hidden file
            fs::write(current.join(".hidden"), "h").unwrap();
            // Binary file
            fs::write(current.join("binary.bin"), vec![0u8, depth as u8]).unwrap();
        }
    }

    let files: Vec<_> = Walker::new()
        .add_root(root)
        .skip_binary(true)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .map(|f| (f.path.clone(), f.is_hidden()))
        .collect();

    // Should find 20 files (10 visible, 10 hidden), skip 10 binary
    assert_eq!(
        files.len(),
        20,
        "Fix: Must find 20 files (10 visible + 10 hidden)"
    );

    // Verify hidden detection works
    let hidden_count = files.iter().filter(|(_, h)| *h).count();
    assert_eq!(
        hidden_count, 10,
        "Fix: Must correctly identify 10 hidden files"
    );
}
