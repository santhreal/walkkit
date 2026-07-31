#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::fs;
use std::path::Path;
use std::thread;
use tempfile::tempdir;
use walkkit::{FileFilter, Walker};

// 1. Empty input / zero-length slices
#[test]
fn test_01_empty_filter_include() {
    let filter = FileFilter::new().add_include("");
    let res = filter.compile();
    assert!(
        res.is_err(),
        "Empty include pattern should fail compilation or be explicitly rejected"
    );
}

#[test]
fn test_02_empty_filter_exclude() {
    let filter = FileFilter::new().add_exclude("");
    let res = filter.compile();
    assert!(
        res.is_err(),
        "Empty exclude pattern should fail compilation or be explicitly rejected"
    );
}

#[test]
fn test_03_empty_extension_filter() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("test_file_no_ext"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).with_extension_filter("");
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "Empty extension filter should exactly match files without any extension"
    );
}

#[test]
fn test_04_empty_root_path() {
    let walker = Walker::new().add_root("");
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(
        files.is_empty(),
        "Empty root path should not panic and should yield 0 files"
    );
}

// 2. Null bytes in input
#[test]
fn test_05_null_byte_in_root() {
    let walker = Walker::new().add_root("test\0dir");
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(
        files.is_empty(),
        "Null byte in root path should be rejected and yield 0 files"
    );
}

#[test]
fn test_06_null_byte_in_include_pattern() {
    let filter = FileFilter::new().add_include("*\0.rs");
    let res = filter.compile();
    assert!(
        res.is_err(),
        "Null byte in include pattern should return an explicit error"
    );
}

#[test]
fn test_07_null_byte_in_exclude_pattern() {
    let filter = FileFilter::new().add_exclude("*\0.rs");
    let res = filter.compile();
    assert!(
        res.is_err(),
        "Null byte in exclude pattern should return an explicit error"
    );
}

#[test]
fn test_08_null_byte_in_extension_filter() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("test.txt"), "data").unwrap();
    let walker = Walker::new()
        .add_root(dir.path())
        .with_extension_filter("txt\0");
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(
        files.is_empty(),
        "Null byte in extension filter should safely yield 0 files"
    );
}

// 3. Maximum u32/u64 values for any numeric parameter
#[test]
fn test_09_max_parallelism() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "a").unwrap();
    let walker = Walker::new()
        .add_root(dir.path())
        .with_parallelism(usize::MAX);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "Walker should handle usize::MAX parallelism gracefully without panic"
    );
}

#[test]
fn test_10_max_depth() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "a").unwrap();
    let walker = Walker::new()
        .add_root(dir.path())
        .with_max_depth(usize::MAX);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "Walker should handle usize::MAX depth gracefully without panic"
    );
}

#[test]
fn test_11_max_size_limit() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "a").unwrap();
    let walker = Walker::new().add_root(dir.path()).with_size_limit(u64::MAX);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "Walker should handle u64::MAX size limit gracefully without panic"
    );
}

#[test]
fn test_12_zero_parallelism_defaults_to_one() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "a").unwrap();
    let walker = Walker::new().add_root(dir.path()).with_parallelism(0);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "Walker should safely default 0 parallelism to at least 1"
    );
}

// 4. 1MB+ input (if the crate processes byte buffers)
#[test]
fn test_13_large_binary_file_skipping() {
    let dir = tempdir().unwrap();
    let mut buf = vec![1u8; 1024 * 1024]; // 1MB
    // Binary detection samples the first 64 KiB (git/ripgrep heuristic), so the
    // NUL that marks this file binary must sit within that prefix. A NUL only
    // past the prefix is a documented non-detection (covered by
    // test_adv_binary_nul_past_prefix_is_treated_as_text).
    buf[1024] = 0;
    fs::write(dir.path().join("large.bin"), &buf).unwrap();

    let walker = Walker::new().add_root(dir.path()).skip_binary(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(
        files.is_empty(),
        "a 1MB binary file with a NUL in the first 64 KiB must be skipped"
    );
}

#[test]
fn test_14_large_text_file_not_skipped() {
    let dir = tempdir().unwrap();
    let buf = vec![b'a'; 1024 * 1024]; // 1MB text
    fs::write(dir.path().join("large.txt"), &buf).unwrap();

    let walker = Walker::new().add_root(dir.path()).skip_binary(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "Walker should not mistakenly skip large pure text files"
    );
}

// 5. Concurrent access from 8 threads (if the crate has shared state)
#[test]
fn test_15_concurrent_walker_execution() {
    let dir = tempdir().unwrap();
    for i in 0..10 {
        fs::write(dir.path().join(format!("{i}.txt")), "data").unwrap();
    }

    let mut handles = vec![];
    for _ in 0..8 {
        let path = dir.path().to_path_buf();
        handles.push(thread::spawn(move || {
            let walker = Walker::new().add_root(path).with_parallelism(2);
            let files: Vec<_> = walker
                .walk()
                .unwrap()
                .filter_map(walkkit::WalkItem::into_file)
                .collect();
            assert_eq!(
                files.len(),
                10,
                "Concurrent walk executions should consistently find all 10 files"
            );
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_16_concurrent_filter_compilation() {
    let mut handles = vec![];
    for _ in 0..8 {
        handles.push(thread::spawn(move || {
            let filter = FileFilter::new()
                .add_include("*.rs")
                .add_exclude("target/**");
            let compiled = filter.compile().unwrap();
            assert!(
                compiled.is_match(Path::new("src/main.rs")),
                "Compiled filter must match exactly across concurrent access"
            );
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }
}

// 6. Malformed/truncated input (partial data, missing headers)
#[test]
fn test_17_malformed_gitignore_handling() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".gitignore"), "/*\n!/foo\n\\x00").unwrap();
    fs::write(dir.path().join("test.txt"), "data").unwrap();

    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(
        files.len() <= 1,
        "Walker should handle malformed .gitignore seamlessly without panicking"
    );
}

#[test]
fn test_18_malformed_symlink_loop() {
    let dir = tempdir().unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path(), dir.path().join("loop")).unwrap();
    }
    #[cfg(not(unix))]
    {
        fs::write(dir.path().join("dummy.txt"), "data").unwrap();
    }
    let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    #[cfg(unix)]
    assert!(
        files.is_empty(),
        "Walker should correctly detect and prevent infinite symlink loops"
    );
    #[cfg(not(unix))]
    assert_eq!(files.len(), 1, "Walker should behave correctly natively");
}

#[test]
fn test_19_truncated_file_during_walk() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("trunc.txt");
    fs::write(&file_path, "12345").unwrap();

    let walker = Walker::new().add_root(dir.path());
    let rx = walker.try_walk_parallel().unwrap();

    fs::File::create(&file_path).unwrap().set_len(0).unwrap();

    let files: Vec<_> = rx
        .into_iter()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "Walker must gracefully process dynamically truncated files during walk"
    );
}

// 7. Unicode edge cases (BOM, overlong sequences, surrogates)
#[test]
fn test_20_unicode_root_path() {
    let dir = tempdir().unwrap();
    let unicode_dir = dir.path().join("🌟dir");
    fs::create_dir(&unicode_dir).unwrap();
    fs::write(unicode_dir.join("file.txt"), "data").unwrap();

    let walker = Walker::new().add_root(&unicode_dir);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "Walker should cleanly process unicode root paths"
    );
}

#[test]
fn test_21_unicode_extension() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("file.🦀"), "data").unwrap();

    let walker = Walker::new()
        .add_root(dir.path())
        .with_extension_filter("🦀");
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "Walker must correctly filter by unicode extensions"
    );
}

#[test]
fn test_22_unicode_filter_pattern() {
    let filter = FileFilter::new().add_include("*.🦀");
    let compiled = filter.compile().unwrap();
    assert!(
        compiled.is_match(Path::new("test.🦀")),
        "Walker must support unicode accurately in filter patterns"
    );
}

#[test]
fn test_23_invalid_utf8_filename() {
    let dir = tempdir().unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let invalid_path = dir
            .path()
            .join(std::ffi::OsStr::from_bytes(&[0xFF, 0xFE, 0xFD]));
        fs::write(&invalid_path, "data").unwrap();
    }
    #[cfg(not(unix))]
    {
        fs::write(dir.path().join("valid.txt"), "data").unwrap();
    }

    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(
        files.len(),
        1,
        "Walker must handle invalid utf8 filenames transparently without panicking"
    );
}

// 8. Duplicate entries (same key twice, same pattern twice)
#[test]
fn test_24_duplicate_roots() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "a").unwrap();

    let walker = Walker::new().add_root(dir.path()).add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "Walker must strictly deduplicate duplicate root paths to avoid duplicate results"
    );
}

#[test]
fn test_25_duplicate_include_patterns() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "a").unwrap();

    let filter = FileFilter::new().add_include("*.txt").add_include("*.txt");
    let walker = Walker::new().add_root(dir.path()).with_filter(filter);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "Walker should handle duplicate include patterns seamlessly"
    );
}

#[test]
fn test_26_duplicate_exclude_patterns() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("a.txt"), "a").unwrap();

    let filter = FileFilter::new().add_exclude("*.txt").add_exclude("*.txt");
    let walker = Walker::new().add_root(dir.path()).with_filter(filter);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(
        files.is_empty(),
        "Walker should handle duplicate exclude patterns flawlessly"
    );
}

// 9. Off-by-one: first byte, last byte, boundary between chunks
#[test]
fn test_27_nul_at_exactly_8191_binary_check() {
    let dir = tempdir().unwrap();
    let mut buf = vec![b'a'; 8192];
    buf[8191] = 0; // Exactly at the end of the 8192 chunk
    fs::write(dir.path().join("bound1.bin"), &buf).unwrap();

    let walker = Walker::new().add_root(dir.path()).skip_binary(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(
        files.is_empty(),
        "NUL byte at exact boundary 8191 must cleanly skip the file"
    );
}

#[test]
fn test_28_nul_at_exactly_8192_binary_check() {
    let dir = tempdir().unwrap();
    let mut buf = vec![b'a'; 8193];
    buf[8192] = 0; // Exactly one byte past the 8192 chunk
    fs::write(dir.path().join("bound2.bin"), &buf).unwrap();

    let walker = Walker::new().add_root(dir.path()).skip_binary(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(files.is_empty(), "NUL byte immediately past chunk boundary 8192 must skip the file, avoiding strict boundary limits");
}

#[test]
fn test_29_size_limit_exact_match() {
    let dir = tempdir().unwrap();
    let buf = vec![b'x'; 100];
    fs::write(dir.path().join("exact.txt"), &buf).unwrap();

    let walker = Walker::new().add_root(dir.path()).with_size_limit(100);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "File exactly at maximum size limit should be firmly included"
    );
}

#[test]
fn test_30_size_limit_off_by_one() {
    let dir = tempdir().unwrap();
    let buf = vec![b'x'; 101];
    fs::write(dir.path().join("over.txt"), &buf).unwrap();

    let walker = Walker::new().add_root(dir.path()).with_size_limit(100);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(
        files.is_empty(),
        "File precisely 1 byte over maximum size limit should be absolutely excluded"
    );
}

#[test]
fn test_31_depth_limit_exact_match() {
    let dir = tempdir().unwrap();
    let nested = dir.path().join("a");
    fs::create_dir(&nested).unwrap();
    fs::write(nested.join("file.txt"), "data").unwrap();

    let walker = Walker::new().add_root(dir.path()).with_max_depth(1);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "File exactly at max depth boundary should be strictly included"
    );
}

// 10. Resource exhaustion: 100K items, deeply nested structures
#[test]
fn test_32_deeply_nested_directories() {
    let dir = tempdir().unwrap();
    let mut current = dir.path().to_path_buf();
    for _ in 0..100 {
        current = current.join("nested");
        fs::create_dir(&current).unwrap();
    }
    fs::write(current.join("deep.txt"), "data").unwrap();

    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(
        files.len(),
        1,
        "Walker should traverse immensely deep recursive structures without risking stack overflow"
    );
}

#[test]
fn test_33_excessive_roots() {
    let mut walker = Walker::new();
    for i in 0..10_000 {
        walker = walker.add_root(format!("/fake/root/{}", i));
    }
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(
        files.is_empty(),
        "Walker must rapidly process gigantic volumes of non-existent roots without failing"
    );
}
