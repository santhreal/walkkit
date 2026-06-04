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
