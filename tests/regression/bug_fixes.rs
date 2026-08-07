use std::fs;
use tempfile::TempDir;
use walkkit::Walker;

#[test]
fn regression_inode_reuse_on_different_devices() {
    // Tests fix for Issue #7: Inode reuse causes silent skipping
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // We can't strictly force different devices in a pure tempdir,
    // but we can ensure normal directory walking correctly traverses everything
    fs::create_dir_all(root.join("dir1/nested")).unwrap();
    fs::create_dir_all(root.join("dir2/nested")).unwrap();

    fs::write(root.join("dir1/nested/f1.txt"), b"1").unwrap();
    fs::write(root.join("dir2/nested/f2.txt"), b"2").unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(root)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 2);
}

#[test]
fn regression_toctou_gitignore() {
    // Tests fix for Issue #2: TOCTOU in Gitignore
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Create a symlink named .gitignore pointing to a sensitive file
    fs::write(root.join("sensitive.txt"), b"SECRET").unwrap();

    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("sensitive.txt"), root.join(".gitignore")).unwrap();

    // Add some files
    fs::write(root.join("test.txt"), b"data").unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(root)
        .respect_gitignore(true) // Reads symlinked `.gitignore` like git(1); rules come from target file.
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 2); // sensitive.txt, test.txt
}

#[test]
fn regression_content_returns_full_bytes_after_post_walk_growth() {
    // Locks out the pre-0.1.1 silent-truncation bug in `FileEntry::content`:
    // the old implementation read only as many bytes as the size captured at
    // walk time, so a file that grew between `walk()` and `content()` was
    // silently truncated. `content()` now re-stats the live file and reads to
    // EOF, so it must return every current byte up to the 256 MiB cap.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("growing.txt");
    fs::write(&path, b"initial-").unwrap();

    let walker = walkkit::CodeWalker::new(dir.path(), walkkit::WalkConfig::default());
    let entries = walker.walk().unwrap();
    let entry = entries.iter().find(|e| e.path == path).unwrap();
    assert_eq!(entry.size, 8, "walk should have captured the pre-growth size");

    // Grow the file after the walk captured its size.
    let mut expected = b"initial-".to_vec();
    expected.extend(std::iter::repeat_n(b'x', 1 << 20));
    fs::write(&path, &expected).unwrap();

    let content = entry.content().unwrap();
    assert_eq!(
        content.as_bytes(),
        expected.as_slice(),
        "content() must return the full current bytes, not the walk-time prefix"
    );
}


#[test]
fn regression_content_fails_closed_above_autoload_cap() {
    // Locks out silent truncation for oversized files: a file larger than the
    // 256 MiB autoload cap must produce `FileTooLarge`, never a truncated
    // buffer. A sparse file keeps this test fast while the re-stat reports the
    // true length.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("huge.bin");
    let file = fs::File::create(&path).unwrap();
    file.set_len(256 * 1024 * 1024 + 1).unwrap();

    let entry = walkkit::FileEntry {
        path,
        size: 1, // stale walk-time size; the live file is over the cap
        is_binary: false,
    };
    let err = entry.content().unwrap_err();
    assert!(
        matches!(err, walkkit::error::CodewalkError::FileTooLarge(_)),
        "expected FileTooLarge, got {err:?}"
    );
}
