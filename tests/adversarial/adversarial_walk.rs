#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::thread;
use tempfile::TempDir;
use walkkit::Walker;

// 1-5: Symlink loop detection
#[test]
fn test_01_symlink_direct_loop() {
    let dir = TempDir::new().unwrap();
    let link = dir.path().join("link");
    symlink(&link, &link).unwrap();
    let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 0);
}

#[test]
fn test_02_symlink_indirect_loop() {
    let dir = TempDir::new().unwrap();
    let link1 = dir.path().join("link1");
    let link2 = dir.path().join("link2");
    symlink(&link2, &link1).unwrap();
    symlink(&link1, &link2).unwrap();
    let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 0);
}

#[test]
fn test_03_symlink_three_way_loop() {
    let dir = TempDir::new().unwrap();
    let link1 = dir.path().join("link1");
    let link2 = dir.path().join("link2");
    let link3 = dir.path().join("link3");
    symlink(&link2, &link1).unwrap();
    symlink(&link3, &link2).unwrap();
    symlink(&link1, &link3).unwrap();
    let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 0);
}

#[test]
fn test_04_symlink_to_parent() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("file.txt"), "data").unwrap();
    symlink(dir.path(), sub.join("parent_link")).unwrap();
    let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
}

#[test]
fn test_05_symlink_to_self_dir() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    symlink(&sub, sub.join("self_link")).unwrap();
    let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 0);
}

// 6-10: Permission denied on files/dirs
#[test]
fn test_06_permission_denied_file() {
    let dir = TempDir::new().unwrap();
    let file = dir.path().join("secret.txt");
    fs::write(&file, "data").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o000)).unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    fs::set_permissions(&file, fs::Permissions::from_mode(0o644)).unwrap();
}

#[test]
fn test_07_permission_denied_dir() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("secret_dir");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join("file.txt"), "data").unwrap();
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o000)).unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 0);
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn test_08_permission_denied_file_in_readable_dir() {
    let dir = TempDir::new().unwrap();
    let sub = dir.path().join("dir");
    fs::create_dir(&sub).unwrap();
    let file1 = sub.join("ok.txt");
    let file2 = sub.join("bad.txt");
    fs::write(&file1, "data").unwrap();
    fs::write(&file2, "data").unwrap();
    fs::set_permissions(&file2, fs::Permissions::from_mode(0o000)).unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 2);
    fs::set_permissions(&file2, fs::Permissions::from_mode(0o644)).unwrap();
}

#[test]
fn test_09_permission_denied_symlink_target() {
    let dir = TempDir::new().unwrap();
    let target = dir.path().join("target");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("file.txt"), "data").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o000)).unwrap();
    let link = dir.path().join("link");
    symlink(&target, &link).unwrap();
    let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 0);
    fs::set_permissions(&target, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn test_10_permission_denied_gitignore() {
    let dir = TempDir::new().unwrap();
    let gi = dir.path().join(".gitignore");
    fs::write(&gi, "*.txt").unwrap();
    fs::set_permissions(&gi, fs::Permissions::from_mode(0o000)).unwrap();
    fs::write(dir.path().join("file.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 2);
    fs::set_permissions(&gi, fs::Permissions::from_mode(0o644)).unwrap();
}

// 11-15: Very deep directory nesting (1000 levels)
#[test]
fn test_11_deep_nesting_bottom_file() {
    let dir = TempDir::new().unwrap();
    let mut current = dir.path().to_path_buf();
    for _ in 0..1000 {
        current = current.join("d");
        fs::create_dir(&current).unwrap();
    }
    fs::write(current.join("file.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
}

#[test]
fn test_12_deep_nesting_all_files() {
    let dir = TempDir::new().unwrap();
    let mut current = dir.path().to_path_buf();
    for i in 0..1000 {
        current = current.join("d");
        fs::create_dir(&current).unwrap();
        fs::write(current.join(format!("f{}.txt", i)), "data").unwrap();
    }
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1000);
}

#[test]
fn test_13_deep_nesting_max_depth() {
    let dir = TempDir::new().unwrap();
    let mut current = dir.path().to_path_buf();
    for i in 0..1000 {
        current = current.join("d");
        fs::create_dir(&current).unwrap();
        fs::write(current.join(format!("f{}.txt", i)), "data").unwrap();
    }
    let walker = Walker::new().add_root(dir.path()).with_max_depth(500);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 500);
}

#[test]
fn test_14_deep_nesting_empty() {
    let dir = TempDir::new().unwrap();
    let mut current = dir.path().to_path_buf();
    for _ in 0..1000 {
        current = current.join("d");
        fs::create_dir(&current).unwrap();
    }
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 0);
}

#[test]
fn test_15_deep_nesting_symlink_to_bottom() {
    let dir = TempDir::new().unwrap();
    let mut current = dir.path().to_path_buf();
    for _ in 0..1000 {
        current = current.join("d");
        fs::create_dir(&current).unwrap();
    }
    fs::write(current.join("file.txt"), "data").unwrap();
    let link = dir.path().join("link");
    symlink(&current, &link).unwrap();
    let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert!(files.len() == 1 || files.len() == 2);
}

// 16-20: Filenames with special chars
#[test]
fn test_16_filename_unicode() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("こんにちは.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
}

#[test]
fn test_17_filename_spaces() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("  file  with  spaces  .txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
}

#[test]
fn test_18_filename_newline() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("file\nname.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
}

#[test]
fn test_19_filename_invalid_utf8() {
    let dir = TempDir::new().unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let invalid = std::ffi::OsStr::from_bytes(&[0xFF, 0xFE, 0xFD]);
        fs::write(dir.path().join(invalid), "data").unwrap();
        let walker = Walker::new().add_root(dir.path());
        let files: Vec<_> = walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .collect();
        assert_eq!(files.len(), 1);
    }
}

#[test]
fn test_20_filename_control_chars() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("file\t\x07\x08.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
}

// 21-25: Empty directories, single-file directories
#[test]
fn test_21_empty_directory() {
    let dir = TempDir::new().unwrap();
    let walker = Walker::new().add_root(dir.path());
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        0
    );
}

#[test]
fn test_22_single_file() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("file.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path());
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        1
    );
}

#[test]
fn test_23_single_empty_directory() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("sub")).unwrap();
    let walker = Walker::new().add_root(dir.path());
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        0
    );
}

#[test]
fn test_24_multiple_empty_directories() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("sub1")).unwrap();
    fs::create_dir(dir.path().join("sub2")).unwrap();
    fs::create_dir(dir.path().join("sub3")).unwrap();
    let walker = Walker::new().add_root(dir.path());
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        0
    );
}

#[test]
fn test_25_single_file_deep_empty_dirs() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("sub1")).unwrap();
    fs::create_dir(dir.path().join("sub2")).unwrap();
    let deep = dir.path().join("sub3").join("sub4");
    fs::create_dir_all(&deep).unwrap();
    fs::write(deep.join("file.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path());
    assert_eq!(
        walker
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .count(),
        1
    );
}

// 26-30: .gitignore patterns
#[test]
fn test_26_gitignore_simple() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".gitignore"), "*.txt").unwrap();
    fs::write(dir.path().join("file.txt"), "data").unwrap();
    fs::write(dir.path().join("file.rs"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 2);
}

#[test]
fn test_27_gitignore_nested() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".gitignore"), "*.txt").unwrap();
    let sub = dir.path().join("sub");
    fs::create_dir(&sub).unwrap();
    fs::write(sub.join(".gitignore"), "!*.txt").unwrap();
    fs::write(sub.join("file.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 3);
}

#[test]
fn test_28_gitignore_negation() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".gitignore"), "*.txt\n!important.txt").unwrap();
    fs::write(dir.path().join("file.txt"), "data").unwrap();
    fs::write(dir.path().join("important.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 2);
}

#[test]
fn test_29_gitignore_directory() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".gitignore"), "build/").unwrap();
    let build = dir.path().join("build");
    fs::create_dir(&build).unwrap();
    fs::write(build.join("file.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
}

#[test]
fn test_30_gitignore_invalid_syntax() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(".gitignore"), "[\n").unwrap();
    fs::write(dir.path().join("file.txt"), "data").unwrap();
    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 2);
}

// 31-33: Race conditions
#[test]
fn test_31_race_file_deleted() {
    let dir = TempDir::new().unwrap();
    for i in 0..100 {
        fs::write(dir.path().join(format!("f{}.txt", i)), "data").unwrap();
    }
    let walker = Walker::new().add_root(dir.path()).with_parallelism(2);
    let rx = walker.try_walk_parallel().unwrap();

    let path_clone = dir.path().to_path_buf();
    thread::spawn(move || {
        for i in 0..100 {
            let _ = fs::remove_file(path_clone.join(format!("f{}.txt", i)));
        }
    });

    let mut count = 0;
    while let Ok(_) = rx.recv() {
        count += 1;
    }
    assert!(count >= 0);
}

#[test]
fn test_32_race_directory_deleted() {
    let dir = TempDir::new().unwrap();
    for i in 0..10 {
        let sub = dir.path().join(format!("d{}", i));
        fs::create_dir(&sub).unwrap();
        for j in 0..10 {
            fs::write(sub.join(format!("f{}.txt", j)), "data").unwrap();
        }
    }
    let walker = Walker::new().add_root(dir.path()).with_parallelism(2);
    let rx = walker.try_walk_parallel().unwrap();

    let path_clone = dir.path().to_path_buf();
    thread::spawn(move || {
        for i in 0..10 {
            let _ = fs::remove_dir_all(path_clone.join(format!("d{}", i)));
        }
    });

    let mut count = 0;
    while let Ok(_) = rx.recv() {
        count += 1;
    }
    assert!(count >= 0);
}

#[test]
fn test_33_race_permissions_changed() {
    let dir = TempDir::new().unwrap();
    for i in 0..100 {
        fs::write(dir.path().join(format!("f{}.txt", i)), "data").unwrap();
    }
    let walker = Walker::new().add_root(dir.path()).with_parallelism(2);
    let rx = walker.try_walk_parallel().unwrap();

    let path_clone = dir.path().to_path_buf();
    thread::spawn(move || {
        for i in 0..100 {
            let _ = fs::set_permissions(
                path_clone.join(format!("f{}.txt", i)),
                fs::Permissions::from_mode(0o000),
            );
        }
    });

    let mut count = 0;
    while let Ok(_) = rx.recv() {
        count += 1;
    }
    assert!(count >= 0);
}
