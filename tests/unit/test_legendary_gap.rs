#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::fs;
use std::os::unix::fs::symlink;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;
use walkkit::Walker;

#[test]
fn test_gap_concurrent_mutation_during_walk() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let root = temp.path().to_path_buf();

    // Create initial files
    for i in 0..100 {
        fs::write(root.join(format!("file_{}.txt", i)), "data")?;
    }

    // Start a walker but don't consume immediately
    let walker = Walker::new().add_root(root.clone()).with_parallelism(2);
    let rx = walker.try_walk_parallel()?;

    // In another thread, aggressively delete and recreate files to cause TOCTOU (Time of Check, Time of Use)
    let root_clone = root.clone();
    let handler = thread::spawn(move || {
        for i in 0..100 {
            let path = root_clone.join(format!("file_{}.txt", i));
            // Ignore errors if file is locked or doesn't exist
            let _ = fs::remove_file(&path);
            let _ = fs::write(&path, "new_data");
        }
    });

    // Consume while mutation happens. The expectation is NO panics, NO unhandled errors crashing the walker,
    // just graceful skips or successful reads of either version.
    let mut files_found = 0;
    while let Ok(item) = rx.recv() {
        // Just draining to ensure no internal panic in walkkit
        // and simulating work by small delays to overlap with mutator
        thread::sleep(Duration::from_micros(10));
        if item.into_file().is_some() {
            files_found += 1;
        }
    }

    handler.join().unwrap();

    // We expect to find *some* files, but we mainly care it doesn't crash.
    assert!(
        files_found > 0,
        "Walker should not completely fail on concurrent mutation"
    );
    Ok(())
}

#[test]
fn test_gap_symlink_size_limit_combination() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let root = temp.path().to_path_buf();

    let target = temp.path().join("target.txt");
    fs::write(&target, vec![b'x'; 2000])?; // 2000 bytes

    let link = root.join("link.txt");
    symlink(&target, &link)?;

    // The link *itself* has a size (usually length of the target path, say 20-30 bytes).
    // The target is 2000 bytes.
    // If we set a size limit of 100 bytes and follow symlinks, it SHOULD filter out the file
    // because the *followed* file size is 2000. If walkkit uses the link size instead, it's a bug.

    let walker = Walker::new()
        .add_root(root.clone())
        .follow_symlinks(true)
        .with_size_limit(100); // 100 bytes max

    let rx = walker.try_walk_parallel()?;

    let mut found = false;
    while let Ok(item) = rx.recv() {
        if let Some(f) = item.into_file() {
            if f.path.file_name().unwrap() == "link.txt"
                || f.path.file_name().unwrap() == "target.txt"
            {
                found = true;
            }
        }
    }

    // Gap expectation: it should correctly read the target size and filter it out
    assert!(
        !found,
        "Symlink target size should be respected for filtering, not the link size itself"
    );

    Ok(())
}

#[test]
fn test_gap_case_insensitivity_mixed_extension_filters() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let root = temp.path().to_path_buf();

    fs::write(root.join("test.JsOn"), "{}")?;
    fs::write(root.join("test.JSON"), "{}")?;
    fs::write(root.join("test.json"), "{}")?;

    // Walkkit currently does exact string match for extensions (case-sensitive) unless documented otherwise.
    // At internet scale, .JSON and .json are the same risk. The walker SHOULD support case-insensitive
    // extension matching, but if it doesn't, this test will fail and serve as a finding.

    let walker = Walker::new()
        .add_root(root.clone())
        .with_extension_filter("json"); // lowercase

    let rx = walker.try_walk_parallel()?;

    let mut count = 0;
    while let Ok(item) = rx.recv() {
        if item.into_file().is_some() {
            count += 1;
        }
    }

    // GAP: Does it find all 3? If the engine only finds 1, it's a finding.
    // We assert it SHOULD find all 3 to be robust.
    // Note: We might need to adjust this if the API explicitly documents case-sensitivity,
    // but internet scale demands robustness.
    // Actually, checking standard implementation, it probably only finds 1.
    // Let's test the gap!
    assert_eq!(
        count, 3,
        "Engine should ideally handle case-insensitive extensions for internet scale scanning"
    );

    Ok(())
}
