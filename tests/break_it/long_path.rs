use walkkit::Walker;

/// A root path longer than `MAX_WALK_PATH_BYTES` (Linux `PATH_MAX`, 4096) cannot
/// be resolved by the absolute-path syscalls the walker uses, so it must be
/// rejected early with the walker's own `InvalidInput` error rather than deferring
/// to the kernel's `ENAMETOOLONG`. The guard runs before any filesystem call, so
/// the path need not (and here cannot) exist on disk.
///
/// Regression guard: with the previous 8192-byte limit a ~4400-byte path slipped
/// past the walker's check and hit the OS error instead.
#[test]
fn path_over_4096_bytes() {
    // ~4401 bytes: greater than PATH_MAX (4096), less than the old 8192 limit.
    let long_root = format!("/{}", "a/".repeat(2200));
    assert!(
        long_root.len() > 4096 && long_root.len() < 8192,
        "test path must straddle PATH_MAX and the old limit (len={})",
        long_root.len()
    );

    let walker = Walker::new().add_root(&long_root);
    let items: Vec<_> = walker.walk().unwrap().collect();
    let errors: Vec<_> = items
        .iter()
        .filter(|i| matches!(i, walkkit::WalkItem::Error(_)))
        .collect();
    assert!(
        !errors.is_empty(),
        "walker should surface an error for overly long paths"
    );

    let has_invalid_input = errors.iter().any(|e| {
        if let walkkit::WalkItem::Error(err) = e {
            err.source.kind() == std::io::ErrorKind::InvalidInput
        } else {
            false
        }
    });
    assert!(
        has_invalid_input,
        "walker should reject long paths with its own InvalidInput limit before hitting the OS"
    );
}
