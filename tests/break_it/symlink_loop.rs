use tempfile::tempdir;
use walkkit::Walker;

/// BUG: walker reports a Metadata error for a file-symlink loop (ELOOP)
/// instead of treating it as a non-traversable file and continuing.
#[cfg(unix)]
#[test]
fn symlink_loop_three_node() {
    let dir = tempdir().unwrap();
    let a = dir.path().join("a");
    let b = dir.path().join("b");
    let c = dir.path().join("c");
    std::os::unix::fs::symlink(&b, &a).unwrap();
    std::os::unix::fs::symlink(&c, &b).unwrap();
    std::os::unix::fs::symlink(&a, &c).unwrap();

    let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
    let items: Vec<_> = walker.walk().unwrap().collect();
    let errors: Vec<_> = items
        .iter()
        .filter(|i| matches!(i, walkkit::WalkItem::Error(_)))
        .collect();
    assert!(
        errors.is_empty(),
        "walker should not emit errors for a file-symlink loop: {:?}",
        errors
    );
}
