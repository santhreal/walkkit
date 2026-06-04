#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use tempfile::TempDir;
use walkkit::{FileFilter, SortMode, Walker};

#[test]
fn test_walker_builder_methods_and_defaults() {
    let mut walker = Walker::new();

    // Test defaults implicitly by verifying no panic and valid state
    let rx = walker
        .clone()
        .try_walk_parallel()
        .expect("Default config should compile");
    assert!(rx.recv().is_err(), "Empty walker should yield nothing");

    // Build with all options
    let filter = FileFilter::new().add_include("*.rs").add_exclude("*test*");
    walker = walker
        .add_root("dummy_path")
        .with_filter(filter)
        .with_parallelism(4)
        .with_sort(SortMode::BySize)
        .follow_symlinks(true)
        .respect_gitignore(false)
        .skip_binary(true)
        .with_max_depth(2)
        .with_extension_filter("rs")
        .with_size_limit(1024);

    // Walker clone should preserve all state
    let _cloned = walker.clone();
}

#[test]
fn test_walker_empty_root_dir() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let root = temp.path().to_path_buf();

    let walker = Walker::new().add_root(root);
    let rx = walker.try_walk_parallel()?;

    // Directory itself is not returned, and it's empty
    assert!(rx.recv().is_err(), "Expected no files in empty directory");
    Ok(())
}

#[test]
fn test_walker_single_file_root() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let file_path = temp.path().join("single.txt");
    fs::write(&file_path, b"hello")?;

    let walker = Walker::new().add_root(file_path.clone());
    let rx = walker.try_walk_parallel()?;

    let item = rx.recv().expect("Should yield single item");
    let file = match item {
        walkkit::WalkItem::File(f) => f,
        walkkit::WalkItem::Error(e) => panic!("unexpected walk error: {e}"),
        _ => panic!("unexpected non-file walk item"),
    };
    assert_eq!(file.path, file_path.as_path());
    assert_eq!(file.size, 5);

    assert!(rx.recv().is_err(), "Should yield only one file");
    Ok(())
}

#[test]
fn test_walker_max_depth_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let root = temp.path().to_path_buf();

    // Structure: root/a/b/c/d.txt
    let dir_path = root.join("a").join("b").join("c");
    fs::create_dir_all(&dir_path)?;
    fs::write(dir_path.join("d.txt"), "data")?;

    // depth 0: root only, but root is a dir, so it should yield nothing
    let walker = Walker::new().add_root(root.clone()).with_max_depth(0);
    let rx = walker.try_walk_parallel()?;
    assert!(rx.recv().is_err());

    // walkkit depth counting: root is 0, a is 1, b is 2, c is 3, d.txt is 4.
    // wait, actually root=0, a=1, b=2, c=3, d.txt is 4.
    // Let's print out what actually comes back to adjust bounds
    let walker = Walker::new().add_root(root.clone()).with_max_depth(2);
    let rx = walker.try_walk_parallel()?;
    assert!(rx.recv().is_err(), "depth 2 should not find d.txt");

    // depth 4: should find d.txt (root->a->b->c->d.txt)
    let walker = Walker::new().add_root(root.clone()).with_max_depth(4);
    let rx = walker.try_walk_parallel()?;
    let item = rx.recv().expect("Should find file at depth 4");
    let f = match item {
        walkkit::WalkItem::File(f) => f,
        walkkit::WalkItem::Error(e) => panic!("unexpected walk error: {e}"),
        _ => panic!("unexpected non-file walk item"),
    };
    assert_eq!(f.path.file_name().unwrap(), "d.txt");

    Ok(())
}

#[test]
fn test_walker_size_limit_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let root = temp.path().to_path_buf();

    fs::write(root.join("10_bytes.txt"), vec![b'x'; 10])?;
    fs::write(root.join("11_bytes.txt"), vec![b'y'; 11])?;

    // Limit 10 bytes: should find 10_bytes.txt, filter out 11_bytes.txt
    let walker = Walker::new().add_root(root.clone()).with_size_limit(10);
    let rx = walker.try_walk_parallel()?;

    let item = rx.recv().expect("Should find one file");
    let f = match item {
        walkkit::WalkItem::File(f) => f,
        walkkit::WalkItem::Error(e) => panic!("unexpected walk error: {e}"),
        _ => panic!("unexpected non-file walk item"),
    };
    assert_eq!(f.path.file_name().unwrap(), "10_bytes.txt");
    assert!(rx.recv().is_err(), "Should filter out > 10 bytes");
    Ok(())
}

#[test]
fn test_walker_extension_filter_edge_cases() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let root = temp.path().to_path_buf();

    fs::write(root.join("test.rs"), "rs")?;
    fs::write(root.join("test.RS"), "RS")?;
    fs::write(root.join("test.rs.bak"), "bak")?;
    fs::write(root.join(".rs"), "hidden")?;

    // Extensions are case-insensitive by default in many matchers, but let's check standard behavior
    // walkkit currently doesn't specify case insensitivity for extensions unless globset is used.
    // Assuming standard case-sensitive unless documented otherwise. We'll test with "rs".
    let walker = Walker::new()
        .add_root(root.clone())
        .with_extension_filter("rs");
    let rx = walker.try_walk_parallel()?;

    let mut files = Vec::new();
    while let Ok(item) = rx.recv() {
        if let Some(f) = item.into_file() {
            files.push(f.path.file_name().unwrap().to_string_lossy().to_string());
        }
    }

    // `.rs` has no file stem in unix, it IS the extension or stem depending on implementation.
    // `test.RS` should be ignored if case-sensitive, or included if case-insensitive.
    // At minimum `test.rs` must be there.
    assert!(
        files.contains(&"test.rs".to_string()),
        "Must contain test.rs"
    );
    assert!(
        !files.contains(&"test.rs.bak".to_string()),
        "Must not contain test.rs.bak"
    );

    Ok(())
}

#[test]
fn test_walker_multiple_roots() -> Result<(), Box<dyn std::error::Error>> {
    let temp1 = TempDir::new()?;
    let temp2 = TempDir::new()?;

    fs::write(temp1.path().join("a.txt"), "A")?;
    fs::write(temp2.path().join("b.txt"), "B")?;

    let walker = Walker::new()
        .add_root(temp1.path().to_path_buf())
        .add_root(temp2.path().to_path_buf());

    let rx = walker.try_walk_parallel()?;

    let mut count = 0;
    while let Ok(item) = rx.recv() {
        if item.into_file().is_some() {
            count += 1;
        }
    }

    assert_eq!(count, 2, "Must walk all roots");
    Ok(())
}

#[test]
fn test_walker_unreadable_directory() -> Result<(), Box<dyn std::error::Error>> {
    let temp = TempDir::new()?;
    let unreadable = temp.path().join("unreadable");
    fs::create_dir(&unreadable)?;

    // Change permissions to remove read access
    let mut perms = fs::metadata(&unreadable)?.permissions();
    perms.set_mode(0o000); // no read/write/exec
    fs::set_permissions(&unreadable, perms)?;

    let walker = Walker::new().add_root(temp.path().to_path_buf());
    let rx = walker.try_walk_parallel()?;

    // We expect the walker to gracefully skip or handle the error without panicking
    while rx.recv().is_ok() {
        // Drain walk items (files and traversal errors)
    }

    // Restore permissions so TempDir cleanup doesn't fail
    let mut perms = fs::metadata(&unreadable)?.permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&unreadable, perms)?;

    Ok(())
}
