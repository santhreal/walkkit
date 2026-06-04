use std::fs;
use tempfile::TempDir;
use walkkit::{FileFilter, SortMode, Walker};

#[test]
fn e2e_massive_directory_lifecycle() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    // Build a realistic directory structure
    fs::create_dir_all(root.join("src/models")).unwrap();
    fs::create_dir_all(root.join("src/controllers")).unwrap();
    fs::create_dir_all(root.join("tests/integration")).unwrap();
    fs::create_dir_all(root.join(".git/objects")).unwrap();
    fs::create_dir_all(root.join("build/artifacts")).unwrap();

    // Add files
    fs::write(root.join("src/main.rs"), b"fn main() {}").unwrap();
    fs::write(root.join("src/models/user.rs"), b"struct User {}").unwrap();
    fs::write(root.join("tests/integration/db_test.rs"), b"#[test]").unwrap();
    fs::write(root.join(".gitignore"), b"/build/\n").unwrap();
    fs::write(root.join(".git/config"), b"[core]").unwrap();
    fs::write(root.join("build/artifacts/app.bin"), vec![0u8; 10000]).unwrap();
    fs::write(root.join("README.md"), b"# App").unwrap();

    let filter = FileFilter::new().add_include("**.rs").add_include("**.md");

    let files: Vec<_> = Walker::new()
        .add_root(root)
        .with_filter(filter)
        .respect_gitignore(true)
        .skip_binary(true)
        .with_sort(SortMode::ByName)
        .with_parallelism(4)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    // Should find: src/main.rs, src/models/user.rs, tests/integration/db_test.rs, README.md
    assert_eq!(files.len(), 4);

    // .git should be ignored
    assert!(!files
        .iter()
        .any(|f| f.path.to_string_lossy().contains(".git")));

    // build should be ignored by .gitignore
    assert!(!files
        .iter()
        .any(|f| f.path.to_string_lossy().contains("build")));
}
