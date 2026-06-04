#![allow(clippy::expect_used, clippy::unwrap_used)]

use std::fs;
use tempfile::TempDir;
use walkkit::{FileFilter, SortMode, Walker};

fn create_test_tree() -> TempDir {
    let dir = TempDir::new().unwrap();
    let root = dir.path();

    fs::write(root.join("a.txt"), "hello").unwrap();
    fs::write(root.join("b.rs"), "fn main() {}").unwrap();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("sub/c.txt"), "world").unwrap();
    fs::write(root.join("sub/d.rs"), "fn test() {}").unwrap();
    fs::create_dir(root.join("sub/deep")).unwrap();
    fs::write(root.join("sub/deep/e.txt"), "deep").unwrap();

    dir
}

#[test]
fn walk_single_dir_finds_all_files() {
    let dir = create_test_tree();
    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 5);
}

#[test]
fn walk_empty_dir_yields_nothing() {
    let dir = TempDir::new().unwrap();
    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(files.is_empty());
}

#[test]
fn walk_glob_filter_includes() {
    let dir = create_test_tree();
    let filter = FileFilter::new().add_include("**/*.rs");
    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_filter(filter)
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|f| f.path.extension().unwrap() == "rs"));
}

#[test]
fn walk_glob_filter_excludes() {
    let dir = create_test_tree();
    let filter = FileFilter::new().add_exclude("**/*.rs");
    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_filter(filter)
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 3);
    assert!(files.iter().all(|f| f.path.extension().unwrap() == "txt"));
}

#[test]
fn walk_recursive() {
    let dir = create_test_tree();
    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    // Should find files in root, sub, and sub/deep
    let has_deep = files
        .iter()
        .any(|f| f.path.to_string_lossy().contains("deep"));
    assert!(has_deep);
}

#[test]
fn walk_sort_by_size() {
    let dir = create_test_tree();
    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_sort(SortMode::BySize)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(!files.is_empty());
    for pair in files.windows(2) {
        assert!(pair[0].size <= pair[1].size);
    }
}

#[test]
fn walk_sort_by_name() {
    let dir = create_test_tree();
    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(!files.is_empty());
    for pair in files.windows(2) {
        assert!(pair[0].path <= pair[1].path);
    }
}

#[test]
fn walk_parallel_same_results() {
    let dir = create_test_tree();
    let serial: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_parallelism(1)
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    let parallel: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_parallelism(4)
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(serial.len(), parallel.len());
}

#[test]
fn walk_symlink_skip_by_default() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("real.txt"), "content").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(root.join("real.txt"), root.join("link.txt")).unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(root)
        .follow_symlinks(false)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    // On Unix: should find real.txt but not follow link
    assert!(!files.is_empty());
}

#[test]
fn walk_many_files() {
    let dir = TempDir::new().unwrap();
    for i in 0..100 {
        fs::write(
            dir.path().join(format!("file_{i:03}.txt")),
            format!("data {i}"),
        )
        .unwrap();
    }
    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 100);
}

#[test]
fn walk_deep_nesting() {
    let dir = TempDir::new().unwrap();
    let mut path = dir.path().to_path_buf();
    for i in 0..10 {
        path = path.join(format!("level{i}"));
        fs::create_dir(&path).unwrap();
    }
    fs::write(path.join("bottom.txt"), "deep").unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert!(files[0].path.to_string_lossy().contains("bottom.txt"));
}

#[test]
fn walk_gitignore_skips_git_dir() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("real.txt"), "content").unwrap();
    fs::create_dir(root.join(".git")).unwrap();
    fs::write(root.join(".git/HEAD"), "ref: refs/heads/main").unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(root)
        .respect_gitignore(true)
        .with_parallelism(1) // gitignore support requires single-thread mode
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert!(files[0].path.to_string_lossy().contains("real.txt"));
}

#[test]
fn walk_binary_file_detection() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("text.txt"), "hello world").unwrap();
    fs::write(root.join("binary.bin"), [0u8, 1, 2, 0, 3]).unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(root)
        .skip_binary(true)
        .with_parallelism(1)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert!(files[0].path.to_string_lossy().contains("text.txt"));
}

#[test]
fn walk_cancellation_via_drop() {
    let dir = create_test_tree();
    let mut iter = Walker::new()
        .add_root(dir.path())
        .with_sort(SortMode::Unsorted)
        .walk()
        .unwrap();
    // Take one item then drop the iterator
    let first = iter.next();
    assert!(first.is_some());
    drop(iter);
    // No panic = success
}

#[test]
fn walk_file_size_is_correct() {
    let dir = TempDir::new().unwrap();
    let content = "hello world!";
    fs::write(dir.path().join("sized.txt"), content).unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].size, content.len() as u64);
}

#[test]
fn walk_depth_limit() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    // Create: root/a.txt, root/sub1/b.txt, root/sub1/sub2/c.txt
    fs::write(root.join("a.txt"), "level0").unwrap();
    fs::create_dir(root.join("sub1")).unwrap();
    fs::write(root.join("sub1/b.txt"), "level1").unwrap();
    fs::create_dir(root.join("sub1/sub2")).unwrap();
    fs::write(root.join("sub1/sub2/c.txt"), "level2").unwrap();

    // max_depth=1: root contents + first level subdirectory contents
    let files: Vec<_> = Walker::new()
        .add_root(root)
        .with_max_depth(1)
        .with_parallelism(1)
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    // root(0) is recursed → finds a.txt + sub1/
    // sub1(1) is recursed (1 <= 1) → finds b.txt + sub2/
    // sub2(2) NOT recursed (2 > 1) → c.txt not found
    assert_eq!(files.len(), 2);
    assert!(files
        .iter()
        .any(|f| f.path.to_string_lossy().contains("a.txt")));
    assert!(files
        .iter()
        .any(|f| f.path.to_string_lossy().contains("b.txt")));
    assert!(!files
        .iter()
        .any(|f| f.path.to_string_lossy().contains("c.txt")));
}

#[test]
fn walked_file_is_hidden_for_dotfile() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join(".env");
    fs::write(&path, "secret").unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert!(files[0].is_hidden());
}

#[test]
fn walked_file_is_hidden_for_nested_dot_directory() {
    let dir = TempDir::new().unwrap();
    let hidden_dir = dir.path().join(".cache");
    fs::create_dir(&hidden_dir).unwrap();
    fs::write(hidden_dir.join(".token"), "x").unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert!(files[0].is_hidden());
}

#[test]
fn walked_file_is_not_hidden_for_plain_file() {
    let dir = TempDir::new().unwrap();
    let visible = dir.path().join("visible");
    fs::create_dir(&visible).unwrap();
    fs::write(visible.join("plain.txt"), "x").unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(&visible)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert!(!files[0].is_hidden());
}

#[test]
fn walk_extension_filter_keeps_matching_extension() {
    let dir = create_test_tree();
    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_extension_filter("rs")
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 2);
    assert!(files
        .iter()
        .all(|file| file.path.extension().unwrap() == "rs"));
}

#[test]
fn walk_extension_filter_accepts_leading_dot() {
    let dir = create_test_tree();
    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_extension_filter(".txt")
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 3);
    assert!(files
        .iter()
        .all(|file| file.path.extension().unwrap() == "txt"));
}

#[test]
fn walk_extension_filter_excludes_files_without_extension() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("README"), "docs").unwrap();
    fs::write(dir.path().join("lib.rs"), "fn main() {}").unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_extension_filter("rs")
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("lib.rs"));
}

#[test]
fn walk_size_limit_filters_large_files() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("small.txt"), b"1234").unwrap();
    fs::write(dir.path().join("large.txt"), vec![b'x'; 64]).unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_size_limit(8)
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("small.txt"));
}

#[test]
fn walk_size_limit_zero_keeps_only_empty_files() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("empty.txt"), b"").unwrap();
    fs::write(dir.path().join("nonempty.txt"), b"x").unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_size_limit(0)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("empty.txt"));
}

#[test]
fn walk_parallel_respects_extension_filter() {
    let dir = create_test_tree();
    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_parallelism(4)
        .with_extension_filter("txt")
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 3);
    assert!(files
        .iter()
        .all(|file| file.path.extension().unwrap() == "txt"));
}

#[test]
fn walk_parallel_respects_size_limit() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("small.txt"), b"1234").unwrap();
    fs::write(dir.path().join("large.txt"), vec![b'x'; 128]).unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_parallelism(4)
        .with_size_limit(8)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("small.txt"));
}

#[test]
fn walk_parallel_respects_skip_binary() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("text.txt"), "hello").unwrap();
    fs::write(dir.path().join("binary.bin"), [0u8, 1, 2, 3]).unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_parallelism(4)
        .skip_binary(true)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("text.txt"));
}

#[test]
fn walk_parallel_respects_depth_limit() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("a.txt"), "root").unwrap();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("sub/b.txt"), "child").unwrap();
    fs::create_dir(root.join("sub/deeper")).unwrap();
    fs::write(root.join("sub/deeper/c.txt"), "grandchild").unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(root)
        .with_parallelism(4)
        .with_max_depth(1)
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert!(files.iter().any(|file| file.path.ends_with("a.txt")));
    assert!(files.iter().any(|file| file.path.ends_with("b.txt")));
    assert!(!files.iter().any(|file| file.path.ends_with("c.txt")));
}

#[test]
fn walk_parallel_respects_git_directory_skip() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("plain.txt"), "x").unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    fs::write(dir.path().join(".git/HEAD"), "ref").unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_parallelism(4)
        .respect_gitignore(true)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("plain.txt"));
}

#[test]
fn walk_dirs_only_yields_nothing() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("a")).unwrap();
    fs::create_dir(dir.path().join("a/b")).unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(files.is_empty());
}

#[test]
fn walk_thousand_files_is_complete() {
    let dir = TempDir::new().unwrap();
    for index in 0..1000 {
        fs::write(dir.path().join(format!("file_{index:04}.txt")), b"x").unwrap();
    }

    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_parallelism(4)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 1000);
}

#[test]
fn walk_all_filters_active_find_only_expected_file() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".hidden")).unwrap();
    fs::write(dir.path().join(".hidden/skip.txt"), "x").unwrap();
    fs::write(dir.path().join("match.rs"), "fn main() {}").unwrap();
    fs::write(dir.path().join("toolong.rs"), vec![b'x'; 256]).unwrap();
    fs::write(dir.path().join("binary.rs"), [0u8, 1, 2, 3]).unwrap();
    fs::write(dir.path().join("other.txt"), "txt").unwrap();

    let filter = FileFilter::new().add_exclude("**/.hidden/**");
    let files: Vec<_> = Walker::new()
        .add_root(dir.path())
        .with_filter(filter)
        .with_extension_filter("rs")
        .with_size_limit(64)
        .skip_binary(true)
        .with_parallelism(4)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 1);
    assert!(files[0].path.ends_with("match.rs"));
}

#[test]
fn walk_depth_zero_root_only() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("top.txt"), "root").unwrap();
    fs::create_dir(root.join("sub")).unwrap();
    fs::write(root.join("sub/deep.txt"), "deep").unwrap();

    let files: Vec<_> = Walker::new()
        .add_root(root)
        .with_max_depth(0)
        .with_parallelism(1)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    // Depth 0 means only files directly in root, no subdirectories
    assert_eq!(files.len(), 1);
    assert!(files[0].path.to_string_lossy().contains("top.txt"));
}

#[test]
fn walk_symlink_cycle_does_not_hang() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    fs::write(root.join("file.txt"), "content").unwrap();
    fs::create_dir(root.join("sub")).unwrap();
    // Create a symlink cycle: sub/loop -> root
    #[cfg(unix)]
    std::os::unix::fs::symlink(root, root.join("sub/loop")).unwrap();

    // This must terminate (not infinite loop)
    let files: Vec<_> = Walker::new()
        .add_root(root)
        .follow_symlinks(true)
        .with_parallelism(1)
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    // Should find file.txt but not loop forever
    assert!(files
        .iter()
        .any(|f| f.path.to_string_lossy().contains("file.txt")));
}
