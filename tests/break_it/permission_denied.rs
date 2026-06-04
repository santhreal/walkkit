use std::fs;
use tempfile::tempdir;
use walkkit::Walker;

/// BUG: walker calls metadata_for_path on every child *before* applying
/// filters.  A permission-denied file that is excluded by extension_filter
/// still produces a spurious traversal error.
#[cfg(unix)]
#[test]
fn permission_denied_excluded_file() {
    let dir = tempdir().unwrap();
    let secret = dir.path().join("secret.txt");
    fs::write(&secret, "x").unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&secret).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&secret, perms).unwrap();
    }

    // secret.txt does NOT match .log, yet the walker still stats it.
    let walker = Walker::new()
        .add_root(dir.path())
        .with_extension_filter("log");
    let items: Vec<_> = walker.walk().unwrap().collect();
    let errors: Vec<_> = items
        .iter()
        .filter(|i| matches!(i, walkkit::WalkItem::Error(_)))
        .collect();
    assert!(
        errors.is_empty(),
        "walker should not error on permission-denied files that are excluded by filters"
    );
}
