#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::fs;
#[cfg(unix)]
use std::os::unix::fs::{symlink, PermissionsExt};
use std::thread;
use tempfile::TempDir;
use walkkit::Walker;

// 1. Symlink loop detection
#[test]
fn test_adv_symlink_direct_loop() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a");
    #[cfg(unix)]
    {
        symlink(&a, &a).unwrap();
        let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
        assert_eq!(
            walker
                .walk()
                .unwrap()
                .filter_map(walkkit::WalkItem::into_file)
                .count(),
            0
        );
    }
}

#[test]
fn test_adv_symlink_indirect_loop() {
    let dir = TempDir::new().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    fs::create_dir(&a).unwrap();
    #[cfg(unix)]
    {
        symlink(&a, &b).unwrap();
        symlink(&b, a.join("loop")).unwrap();
        let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
        let count = walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count();
        assert!(count <= 2);
    }
}

#[test]
fn test_adv_symlink_to_parent() {
    let dir = TempDir::new().unwrap();
    let child = dir.path().join("child");
    fs::create_dir(&child).unwrap();
    #[cfg(unix)]
    {
        symlink(dir.path(), child.join("up")).unwrap();
        let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
        let _ = walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count();
    }
}

#[test]
fn test_adv_symlink_to_outside() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();
    fs::write(dir2.path().join("file.txt"), "data").unwrap();
    #[cfg(unix)]
    {
        symlink(dir2.path(), dir1.path().join("link")).unwrap();
        let walker = Walker::new().add_root(dir1.path()).follow_symlinks(true);
        assert_eq!(
            walker
                .walk()
                .unwrap()
                .filter_map(walkkit::WalkItem::into_file)
                .count(),
            1
        );
    }
}

// 2. Gitignore respect
#[test]
fn test_adv_gitignore_basic() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".gitignore"), "foo.txt\n").unwrap();
    fs::write(dir.path().join("foo.txt"), "data").unwrap();
    fs::write(dir.path().join("bar.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 2); // bar.txt, .gitignore
}

#[test]
fn test_adv_gitignore_negation() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".gitignore"), "*.txt\n!keep.txt\n").unwrap();
    fs::write(dir.path().join("foo.txt"), "data").unwrap();
    fs::write(dir.path().join("keep.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 2); // keep.txt, .gitignore
}

#[test]
fn test_adv_gitignore_nested() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".gitignore"), "foo/\n").unwrap();
    let foo = dir.path().join("foo");
    fs::create_dir(&foo).unwrap();
    fs::write(foo.join(".gitignore"), "!bar.txt\n").unwrap();
    fs::write(foo.join("bar.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    // foo is ignored, so nested is ignored
    assert_eq!(files.len(), 1); // .gitignore
}

#[test]
fn test_adv_gitignore_nested_override() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".gitignore"), "*.txt\n").unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join(".gitignore"), "!*.txt\n").unwrap();
    fs::write(sub.join("file.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 3); // .gitignore, sub/.gitignore, sub/file.txt
}

#[test]
fn test_adv_gitignore_malformed() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".gitignore"), "[\n*\n\\x00").unwrap();
    fs::write(dir.path().join("file.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let _ = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .count();
}

#[test]
fn test_adv_gitignore_directory_only() {
    let dir = TempDir::new().unwrap();
    // Trailing `/` matches only directories; a regular file named `foo` must stay visible.
    fs::write(dir.path().join(".gitignore"), "foo/\n").unwrap();
    fs::write(dir.path().join("foo"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 2, "expected .gitignore + file foo");
}

// 3. Binary file skip heuristics
#[test]
fn test_adv_binary_nul_start() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("bin"), b"\x00data").unwrap();
    let walker = Walker::new().add_root(dir.path()).skip_binary(true);
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        0
    );
}

#[test]
fn test_adv_binary_nul_end_8k() {
    let dir = TempDir::new().unwrap();
    let mut data = vec![b'A'; 8192];
    data[8191] = 0;
    fs::write(dir.path().join("bin"), &data).unwrap();
    let walker = Walker::new().add_root(dir.path()).skip_binary(true);
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        0
    );
}

#[test]
fn test_adv_binary_nul_after_8k() {
    let dir = TempDir::new().unwrap();
    let mut data = vec![b'A'; 8193];
    data[8192] = 0;
    fs::write(dir.path().join("bin"), &data).unwrap();
    let walker = Walker::new().add_root(dir.path()).skip_binary(true);
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        0
    );
}

#[test]
fn test_adv_binary_large_no_nul() {
    let dir = TempDir::new().unwrap();
    let data = vec![b'A'; 10 * 1024 * 1024];
    fs::write(dir.path().join("text"), &data).unwrap();
    let walker = Walker::new().add_root(dir.path()).skip_binary(true);
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        1
    );
}

/// Binary detection deliberately scans only the first 64 KiB prefix (the
/// git/ripgrep sampling heuristic), a documented bounded-recall tradeoff. A NUL
/// that sits past that prefix is therefore NOT detected and the file is treated
/// as text. This test pins that intentional contract (it previously asserted the
/// old unbounded full-file scan, which the prefix change deliberately removed).
#[test]
fn test_adv_binary_nul_past_prefix_is_treated_as_text() {
    let dir = TempDir::new().unwrap();
    let mut data = vec![b'A'; 10 * 1024 * 1024 + 1];
    data[10 * 1024 * 1024] = 0; // NUL at 10 MiB, far past the 64 KiB prefix.
    fs::write(dir.path().join("bin"), &data).unwrap();
    let walker = Walker::new().add_root(dir.path()).skip_binary(true);
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        1,
        "a NUL past the 64 KiB scan prefix is not detected (bounded-recall tradeoff)"
    );
}

/// The companion contract: a NUL WITHIN the 64 KiB prefix IS detected, so the
/// binary file is skipped. This locks the boundary the test above depends on.
#[test]
fn test_adv_binary_nul_within_prefix_is_skipped() {
    let dir = TempDir::new().unwrap();
    let mut data = vec![b'A'; 10 * 1024 * 1024];
    data[1024] = 0; // NUL at 1 KiB, well within the 64 KiB prefix.
    fs::write(dir.path().join("bin"), &data).unwrap();
    let walker = Walker::new().add_root(dir.path()).skip_binary(true);
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        0,
        "a NUL within the 64 KiB scan prefix must mark the file binary"
    );
}

// 4. Max file size filtering
#[test]
fn test_adv_size_limit_exact() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("file"), vec![b'A'; 100]).unwrap();
    let walker = Walker::new().add_root(dir.path()).with_size_limit(100);
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        1
    );
}

#[test]
fn test_adv_size_limit_exceeded() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("file"), vec![b'A'; 101]).unwrap();
    let walker = Walker::new().add_root(dir.path()).with_size_limit(100);
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        0
    );
}

#[test]
fn test_adv_size_limit_zero() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("empty"), "").unwrap();
    fs::write(dir.path().join("file"), "A").unwrap();
    let walker = Walker::new().add_root(dir.path()).with_size_limit(0);
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        1
    );
}

// 5. Permission denied handling
#[test]
fn test_adv_permission_denied_file() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("unreadable.txt");
    fs::write(&file, "data").unwrap();
    #[cfg(unix)]
    {
        fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
        let walker = Walker::new().add_root(dir.path());
        let _ = walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count();
        // Restore permissions to allow tempdir cleanup
        fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
    }
}

#[test]
fn test_adv_permission_denied_dir() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("unreadable_dir");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("file.txt"), "data").unwrap();
    #[cfg(unix)]
    {
        fs::set_permissions(&sub, fs::Permissions::from_mode(0o000)).unwrap();
        let walker = Walker::new().add_root(dir.path());
        let _ = walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count();
        // Restore permissions to allow tempdir cleanup
        fs::set_permissions(&sub, fs::Permissions::from_mode(0o755)).unwrap();
    }
}

// 6. Empty directories
#[test]
fn test_adv_empty_directory() {
    let dir = TempDir::new().unwrap();
    let walker = Walker::new().add_root(dir.path());
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        0
    );
}

#[test]
fn test_adv_nested_empty_directories() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("a/b/c");
    fs::create_dir_all(&sub).unwrap();
    let walker = Walker::new().add_root(dir.path());
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        0
    );
}

// 7. Directories with 100K files
#[test]
fn test_adv_10k_files_performance() {
    let dir = TempDir::new().unwrap();
    for i in 0..10_000 {
        fs::write(dir.path().join(format!("{}.txt", i)), "").unwrap();
    }
    let walker = Walker::new().add_root(dir.path());
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        10_000
    );
}

// 8. Special filenames
#[test]
fn test_adv_filename_unicode() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("🚀.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path());
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        1
    );
}

#[test]
fn test_adv_filename_newline() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("file\nname.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path());
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        1
    );
}

#[test]
fn test_adv_filename_invalid_utf8() {
    let dir = TempDir::new().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let invalid = std::ffi::OsStr::from_bytes(&[0xFF, 0xFE, 0xFD]);
        fs::write(dir.path().join(invalid), "data").unwrap();
        let walker = Walker::new().add_root(dir.path());
        assert_eq!(
            walker
                .walk()
                .unwrap()
                .filter_map(walkkit::WalkItem::into_file)
                .count(),
            1
        );
    }
}

// 9. Walk reproducibility
#[test]
fn test_adv_reproducibility() {
    let dir = TempDir::new().unwrap();
    for i in 0..100 {
        fs::write(dir.path().join(format!("{:03}.txt", i)), "").unwrap();
    }
    let walker1 = Walker::new().add_root(dir.path()).with_parallelism(1);
    let walker2 = Walker::new().add_root(dir.path()).with_parallelism(1);
    let files1: Vec<_> = walker1
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .map(|f| f.path)
        .collect();
    let files2: Vec<_> = walker2
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .map(|f| f.path)
        .collect();
    assert_eq!(files1, files2);
}

#[test]
fn test_adv_reproducibility_parallel() {
    let dir = TempDir::new().unwrap();
    for i in 0..100 {
        fs::write(dir.path().join(format!("{:03}.txt", i)), "").unwrap();
    }
    let walker1 = Walker::new().add_root(dir.path()).with_parallelism(4);
    let walker2 = Walker::new().add_root(dir.path()).with_parallelism(4);
    let mut files1: Vec<_> = walker1
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .map(|f| f.path)
        .collect();
    let mut files2: Vec<_> = walker2
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .map(|f| f.path)
        .collect();
    files1.sort();
    files2.sort();
    assert_eq!(files1, files2);
}

// Extra edge cases
#[test]
fn test_adv_multiple_roots() {
    let dir1 = TempDir::new().unwrap();
    let dir2 = TempDir::new().unwrap();
    fs::write(dir1.path().join("a.txt"), "").unwrap();
    fs::write(dir2.path().join("b.txt"), "").unwrap();
    let walker = Walker::new().add_root(dir1.path()).add_root(dir2.path());
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        2
    );
}

#[test]
fn test_adv_max_depth_zero() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "").unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("b.txt"), "").unwrap();
    let walker = Walker::new().add_root(dir.path()).with_max_depth(0);
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        1
    );
}

#[test]
fn test_adv_max_depth_one() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "").unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("b.txt"), "").unwrap();
    let subsub = sub.join("subsub");
    fs::create_dir(&subsub).unwrap();
    fs::write(subsub.join("c.txt"), "").unwrap();
    let walker = Walker::new().add_root(dir.path()).with_max_depth(1);
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        2
    );
}

#[test]
fn test_adv_concurrent_walkers() {
    let dir = TempDir::new().unwrap();
    for i in 0..100 {
        fs::write(dir.path().join(format!("{i}.txt")), "").unwrap();
    }
    let mut handles = vec![];
    for _ in 0..8 {
        let path = dir.path().to_path_buf();
        handles.push(thread::spawn(move || {
            let walker = Walker::new().add_root(path);
            assert_eq!(
                walker
                    .walk()
                    .unwrap()
                    .filter_map(walkkit::WalkItem::into_file)
                    .count(),
                100
            );
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_adv_extension_filter() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.txt"), "").unwrap();
    fs::write(dir.path().join("b.rs"), "").unwrap();
    let walker = Walker::new()
        .add_root(dir.path())
        .with_extension_filter("rs");
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        1
    );
}

#[test]
fn test_adv_large_extension_filter() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.longextension"), "").unwrap();
    let walker = Walker::new()
        .add_root(dir.path())
        .with_extension_filter("longextension");
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        1
    );
}
