use tempfile::tempdir;
use walkkit::Walker;

/// BUG: when skip_binary(false) the walker never opens the file, so a
/// file that is replaced (or deleted) after stat is still emitted with
/// stale metadata.
#[test]
fn file_replaced_between_stat_and_emission() {
    let dir = tempdir().unwrap();
    let file_path = dir.path().join("replace.txt");
    std::fs::write(&file_path, "old").unwrap();

    let walker = Walker::new().add_root(dir.path()).skip_binary(false);
    let rx = walker.try_walk_parallel().unwrap();

    // Overwrite after the walk has started.
    std::fs::write(&file_path, "new content").unwrap();

    let files: Vec<_> = rx
        .into_iter()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    let replaced = files.iter().find(|f| f.path.ends_with("replace.txt"));
    assert!(replaced.is_some(), "walker should find the file");

    // The walker reports the size from the original stat (3 bytes), not the
    // updated size (11 bytes), exposing a TOCTOU bug.
    assert_eq!(
        replaced.unwrap().size,
        11,
        "walker should detect the updated file size"
    );
}
