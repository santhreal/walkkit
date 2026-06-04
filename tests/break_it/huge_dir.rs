use std::time::{Duration, Instant};
use tempfile::tempdir;
use walkkit::Walker;

/// BUG: walker does not bound traversal time for flat directories;
/// 100K files are processed serially in a single worker and can exceed
/// reasonable latency expectations.
#[test]
fn hundred_k_files_timeout() {
    let dir = tempdir().unwrap();
    for i in 0..100_000 {
        std::fs::write(dir.path().join(format!("f{i}.txt")), "x").unwrap();
    }
    let walker = Walker::new().add_root(dir.path());
    let start = Instant::now();
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 100_000, "should find all 100K files");
    assert!(
        start.elapsed() < Duration::from_secs(10),
        "walk of 100K files took too long: {:?}",
        start.elapsed()
    );
}
