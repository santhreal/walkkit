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
use tempfile::TempDir;
use walkkit::Walker;

#[test]
fn test_adversarial_null_byte_in_path() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    // standard string literal for null bytes per instructions
    let bad_path = temp.path().join("evil_\0_dir");

    // the filesystem might reject this directly, so we just pass it to walkkit
    let walker = Walker::new().add_root(bad_path);
    let rx = walker.try_walk_parallel()?;

    // NUL in paths is invalid; walk may disconnect with no items or surface a traversal error.
    match rx.recv() {
        Err(_) => {}
        Ok(walkkit::WalkItem::Error(_)) => {}
        Ok(walkkit::WalkItem::File(_)) => {
            panic!("unexpected file for NUL-containing root path");
        }
    }
    Ok(())
}

#[test]
fn test_adversarial_deep_nesting() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let mut current = temp.path().to_path_buf();

    // Create extremely deep nesting to test recursion limits/stack overflow
    // Usually standard OS supports around 255-1024, let's just do 500
    for i in 0..500 {
        current = current.join(format!("dir_{}", i));
    }

    // We ignore the error if OS limits prevent creating this deep
    let _ = fs::create_dir_all(&current);

    // Write a file at the deepest level we achieved
    if let Ok(_) = fs::write(current.join("deep.txt"), "hello") {
        let walker = Walker::new().add_root(temp.path().to_path_buf());
        let rx = walker.try_walk_parallel()?;

        let mut count = 0;
        while let Ok(item) = rx.recv() {
            if item.into_file().is_some() {
                count += 1;
            }
        }
        assert_eq!(
            count, 1,
            "Should find exactly one file deep in the hierarchy"
        );
    }

    Ok(())
}

#[test]
fn test_adversarial_invalid_symlink_loop() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let root = temp.path().to_path_buf();

    let a = root.join("a");
    let b = root.join("b");

    fs::create_dir(&a)?;
    symlink(&a, &b)?;
    symlink(&b, a.join("loop"))?;

    // Walk the directory, ensuring no infinite loop and no panic
    let walker = Walker::new().add_root(root).follow_symlinks(true);
    let rx = walker.try_walk_parallel()?;

    while let Ok(_) = rx.recv() {
        // Just drain and make sure we don't hang
    }

    Ok(())
}

#[test]
fn test_adversarial_huge_number_of_files() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let root = temp.path().to_path_buf();

    // We create 5000 small files to test channel capacity limits and concurrency
    for i in 0..5000 {
        fs::write(root.join(format!("file_{}.txt", i)), "x")?;
    }

    let walker = Walker::new().add_root(root).with_parallelism(8);
    let rx = walker.try_walk_parallel()?;

    let mut count = 0;
    while let Ok(item) = rx.recv() {
        if item.into_file().is_some() {
            count += 1;
        }
    }

    assert_eq!(
        count, 5000,
        "Should find all 5000 files without dropping any"
    );
    Ok(())
}

#[test]
fn test_adversarial_path_traversal() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let root = temp.path().to_path_buf();

    // Malicious roots
    let walker = Walker::new().add_root(root.join("../../../../../../../etc/passwd"));

    // Should not crash, just fail to find things gracefully if invalid
    let rx = walker.try_walk_parallel()?;
    while let Ok(_) = rx.recv() {}

    Ok(())
}

#[test]
fn test_adversarial_unicode_and_invalid_bytes() -> Result<(), Box<dyn std::error::Error>> {
    use std::os::unix::ffi::OsStringExt;
    let temp = TempDir::new()?;
    let root = temp.path().to_path_buf();

    // valid unicode
    fs::write(root.join("🦀.txt"), "crab")?;

    // invalid bytes (0xFF)
    let invalid_path = root.join(std::ffi::OsString::from_vec(vec![
        0xff, 0xff, b'.', b't', b'x', b't',
    ]));
    fs::write(&invalid_path, "invalid")?;

    let walker = Walker::new().add_root(root);
    let rx = walker.try_walk_parallel()?;

    let mut found_crab = false;
    let mut found_invalid = false;

    while let Ok(item) = rx.recv() {
        if let Some(file) = item.into_file() {
            if file.path.file_name().unwrap() == "🦀.txt" {
                found_crab = true;
            } else {
                found_invalid = true;
            }
        }
    }

    assert!(found_crab, "Should find unicode file");
    assert!(found_invalid, "Should find invalid byte file");

    Ok(())
}

#[test]
fn test_adversarial_integer_overflow_size_limit() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let root = temp.path().to_path_buf();

    fs::write(root.join("test.txt"), "hello")?;

    // Test that using u64::MAX size limit doesn't cause overflows in internal logic
    let walker = Walker::new().add_root(root).with_size_limit(u64::MAX);
    let rx = walker.try_walk_parallel()?;

    let item = rx.recv().expect("Should find file");
    let file = match item {
        walkkit::WalkItem::File(f) => f,
        walkkit::WalkItem::Error(e) => panic!("unexpected walk error: {e}"),
    };
    assert_eq!(file.size, 5);

    Ok(())
}
