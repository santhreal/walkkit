#![allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]
// Auto-generated mass walk samples
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use tempfile::tempdir;
use walkkit::Walker;

#[test]
fn test_walk_directory_with_1000_files_at_5_depth_levels() {
    let dir = tempdir().unwrap();
    let root = dir.path();

    let mut file_count = 0;
    for l1 in 0..2 {
        for l2 in 0..2 {
            for l3 in 0..2 {
                for l4 in 0..2 {
                    for l5 in 0..1 {
                        let mut current_dir = root.to_path_buf();
                        current_dir.push(format!("dir_{l1}"));
                        current_dir.push(format!("dir_{l2}"));
                        current_dir.push(format!("dir_{l3}"));
                        current_dir.push(format!("dir_{l4}"));
                        current_dir.push(format!("dir_{l5}"));
                        fs::create_dir_all(&current_dir).unwrap();

                        for f_idx in 0..63 {
                            if file_count >= 1000 {
                                break;
                            }
                            let file_path = current_dir.join(format!("file_{f_idx}.txt"));
                            fs::write(&file_path, "test").unwrap();
                            file_count += 1;
                        }
                    }
                }
            }
        }
    }
    // ensure we reach 1000
    while file_count < 1000 {
        let file_path = root.join(format!("root_file_{file_count}.txt"));
        fs::write(&file_path, "test").unwrap();
        file_count += 1;
    }

    let walker = Walker::new().add_root(root);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1000);
}

#[test]
fn test_symlink_loop_detection_3_levels() {
    let dir = tempdir().unwrap();
    let l1 = dir.path().join("level1");
    let l2 = l1.join("level2");
    let l3 = l2.join("level3");
    fs::create_dir_all(&l3).unwrap();

    fs::write(l3.join("file.txt"), "content").unwrap();

    #[cfg(unix)]
    symlink(&l1, l3.join("loop_link")).unwrap();

    let walker = Walker::new().add_root(dir.path()).follow_symlinks(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
}

#[test]
fn test_gitignore_filtering_at_3_nested_levels() {
    let dir = tempdir().unwrap();
    let l1 = dir.path().join("level1");
    let l2 = l1.join("level2");
    let l3 = l2.join("level3");
    fs::create_dir_all(&l3).unwrap();

    fs::write(dir.path().join(".gitignore"), "ignore_l1.txt\n").unwrap();
    fs::write(l1.join(".gitignore"), "ignore_l2.txt\n").unwrap();
    fs::write(l2.join(".gitignore"), "ignore_l3.txt\n").unwrap();

    fs::write(l1.join("ignore_l1.txt"), "x").unwrap();
    fs::write(l1.join("keep_l1.txt"), "x").unwrap();

    fs::write(l2.join("ignore_l2.txt"), "x").unwrap();
    fs::write(l2.join("keep_l2.txt"), "x").unwrap();

    fs::write(l3.join("ignore_l3.txt"), "x").unwrap();
    fs::write(l3.join("keep_l3.txt"), "x").unwrap();

    let walker = Walker::new().add_root(dir.path()).respect_gitignore(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    let mut names: Vec<_> = files
        .iter()
        .filter_map(|f| f.path.file_name().and_then(|n| n.to_str()))
        .collect();
    names.sort_unstable();

    // .gitignore are also returned by walker
    let mut expected = vec![
        ".gitignore",
        ".gitignore",
        ".gitignore",
        "keep_l1.txt",
        "keep_l2.txt",
        "keep_l3.txt",
    ];
    expected.sort_unstable();

    assert_eq!(names, expected);
}

#[test]
fn test_binary_file_detection_on_20_different_file_types() {
    let dir = tempdir().unwrap();

    // 10 text files
    fs::write(dir.path().join("text_0.txt"), "hello world").unwrap();
    fs::write(dir.path().join("text_1.md"), "# Hello").unwrap();
    fs::write(dir.path().join("text_2.rs"), "fn main() {}").unwrap();
    fs::write(dir.path().join("text_3.toml"), "[package]").unwrap();
    fs::write(dir.path().join("text_4.json"), "{}").unwrap();
    fs::write(dir.path().join("text_5.yaml"), "key: val").unwrap();
    fs::write(dir.path().join("text_6.csv"), "1,2,3").unwrap();
    fs::write(dir.path().join("text_7.xml"), "<root></root>").unwrap();
    fs::write(dir.path().join("text_8.html"), "<html></html>").unwrap();
    fs::write(dir.path().join("text_9.js"), "console.log();").unwrap();

    // 10 binary files
    fs::write(dir.path().join("bin_0.bin"), [0, 1, 2, 0, 4]).unwrap();
    fs::write(dir.path().join("bin_1.exe"), [0, 5, 0, 8]).unwrap();
    fs::write(dir.path().join("bin_2.dll"), [0, 0]).unwrap();
    fs::write(dir.path().join("bin_3.so"), [1, 0, 3]).unwrap();
    fs::write(dir.path().join("bin_4.pdf"), [0, 9, 8, 7]).unwrap();
    fs::write(
        dir.path().join("bin_5.png"),
        [
            137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1,
            8, 6, 0, 0, 0,
        ],
    )
    .unwrap();
    fs::write(
        dir.path().join("bin_6.jpg"),
        [
            255, 216, 255, 224, 0, 16, 74, 70, 73, 70, 0, 1, 1, 0, 0, 1, 0, 1, 0, 0,
        ],
    )
    .unwrap();
    fs::write(dir.path().join("bin_7.zip"), [80, 75, 3, 4, 0]).unwrap();
    fs::write(dir.path().join("bin_8.tar.gz"), [31, 139, 8, 0, 0]).unwrap();
    fs::write(dir.path().join("bin_9.class"), [202, 254, 186, 190, 0]).unwrap();

    let walker = Walker::new().add_root(dir.path()).skip_binary(true);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 10);
    for file in files {
        let name = file.path.to_string_lossy();
        assert!(name.contains("text_"), "Matched non-text file: {name}");
    }
}

#[test]
fn test_permission_denied_handling() {
    let dir = tempdir().unwrap();
    let restricted = dir.path().join("restricted");
    fs::create_dir_all(&restricted).unwrap();
    fs::write(restricted.join("secret.txt"), "secret").unwrap();
    fs::write(dir.path().join("public.txt"), "public").unwrap();

    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&restricted).unwrap().permissions();
        perms.set_mode(0o000);
        fs::set_permissions(&restricted, perms).unwrap();
    }

    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    // reset permissions so tempdir can be cleaned up
    #[cfg(unix)]
    {
        let mut perms = fs::metadata(&restricted).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&restricted, perms).unwrap();
    }

    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "public.txt"
    );
}

#[test]
fn test_max_file_size_filtering_at_boundary() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("size_99.txt"), vec![b'x'; 99]).unwrap();
    fs::write(dir.path().join("size_100.txt"), vec![b'x'; 100]).unwrap();
    fs::write(dir.path().join("size_101.txt"), vec![b'x'; 101]).unwrap();

    let walker = Walker::new().add_root(dir.path()).with_size_limit(100);
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();

    assert_eq!(files.len(), 2);
    let mut names: Vec<_> = files
        .iter()
        .map(|f| {
            f.path
                .file_name()
                .unwrap_or_else(|| std::process::exit(1))
                .to_str()
                .unwrap_or_else(|| std::process::exit(1))
        })
        .collect();
    names.sort_unstable();
    assert_eq!(names, vec!["size_100.txt", "size_99.txt"]);
}

#[test]
fn test_unicode_filenames() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("测试.txt"), "test").unwrap();
    fs::write(dir.path().join("🌟.txt"), "test").unwrap();
    fs::write(dir.path().join("file.txt"), "test").unwrap();

    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 3);
}

#[test]
fn test_hidden_files() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join(".hidden"), "test").unwrap();
    fs::write(dir.path().join("visible"), "test").unwrap();

    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 2);
}

#[test]
fn test_empty_directories() {
    let dir = tempdir().unwrap();
    fs::create_dir_all(dir.path().join("empty1")).unwrap();
    fs::create_dir_all(dir.path().join("empty2/empty3")).unwrap();
    fs::write(dir.path().join("file.txt"), "test").unwrap();

    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "file.txt"
    );
}

#[test]
fn test_sample_0() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_0.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_0.txt"
    );
}

#[test]
fn test_sample_1() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_1.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_1.txt"
    );
}

#[test]
fn test_sample_2() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_2.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_2.txt"
    );
}

#[test]
fn test_sample_3() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_3.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_3.txt"
    );
}

#[test]
fn test_sample_4() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_4.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_4.txt"
    );
}

#[test]
fn test_sample_5() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_5.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_5.txt"
    );
}

#[test]
fn test_sample_6() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_6.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_6.txt"
    );
}

#[test]
fn test_sample_7() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_7.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_7.txt"
    );
}

#[test]
fn test_sample_8() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_8.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_8.txt"
    );
}

#[test]
fn test_sample_9() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_9.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_9.txt"
    );
}

#[test]
fn test_sample_10() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_10.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_10.txt"
    );
}

#[test]
fn test_sample_11() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_11.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_11.txt"
    );
}

#[test]
fn test_sample_12() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_12.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_12.txt"
    );
}

#[test]
fn test_sample_13() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_13.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_13.txt"
    );
}

#[test]
fn test_sample_14() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_14.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_14.txt"
    );
}

#[test]
fn test_sample_15() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_15.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_15.txt"
    );
}

#[test]
fn test_sample_16() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_16.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_16.txt"
    );
}

#[test]
fn test_sample_17() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_17.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_17.txt"
    );
}

#[test]
fn test_sample_18() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_18.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_18.txt"
    );
}

#[test]
fn test_sample_19() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_19.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_19.txt"
    );
}

#[test]
fn test_sample_20() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_20.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_20.txt"
    );
}

#[test]
fn test_sample_21() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_21.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_21.txt"
    );
}

#[test]
fn test_sample_22() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_22.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_22.txt"
    );
}

#[test]
fn test_sample_23() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_23.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_23.txt"
    );
}

#[test]
fn test_sample_24() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_24.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_24.txt"
    );
}

#[test]
fn test_sample_25() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_25.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_25.txt"
    );
}

#[test]
fn test_sample_26() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_26.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_26.txt"
    );
}

#[test]
fn test_sample_27() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_27.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_27.txt"
    );
}

#[test]
fn test_sample_28() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_28.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_28.txt"
    );
}

#[test]
fn test_sample_29() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_29.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_29.txt"
    );
}

#[test]
fn test_sample_30() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_30.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_30.txt"
    );
}

#[test]
fn test_sample_31() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_31.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_31.txt"
    );
}

#[test]
fn test_sample_32() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_32.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_32.txt"
    );
}

#[test]
fn test_sample_33() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_33.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_33.txt"
    );
}

#[test]
fn test_sample_34() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_34.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_34.txt"
    );
}

#[test]
fn test_sample_35() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_35.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_35.txt"
    );
}

#[test]
fn test_sample_36() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_36.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_36.txt"
    );
}

#[test]
fn test_sample_37() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_37.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_37.txt"
    );
}

#[test]
fn test_sample_38() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_38.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_38.txt"
    );
}

#[test]
fn test_sample_39() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_39.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_39.txt"
    );
}

#[test]
fn test_sample_40() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_40.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_40.txt"
    );
}

#[test]
fn test_sample_41() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_41.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_41.txt"
    );
}

#[test]
fn test_sample_42() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_42.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_42.txt"
    );
}

#[test]
fn test_sample_43() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_43.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_43.txt"
    );
}

#[test]
fn test_sample_44() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_44.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_44.txt"
    );
}

#[test]
fn test_sample_45() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_45.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_45.txt"
    );
}

#[test]
fn test_sample_46() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_46.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_46.txt"
    );
}

#[test]
fn test_sample_47() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_47.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_47.txt"
    );
}

#[test]
fn test_sample_48() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_48.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_48.txt"
    );
}

#[test]
fn test_sample_49() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_49.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_49.txt"
    );
}

#[test]
fn test_sample_50() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_50.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_50.txt"
    );
}

#[test]
fn test_sample_51() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_51.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_51.txt"
    );
}

#[test]
fn test_sample_52() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_52.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_52.txt"
    );
}

#[test]
fn test_sample_53() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_53.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_53.txt"
    );
}

#[test]
fn test_sample_54() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_54.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_54.txt"
    );
}

#[test]
fn test_sample_55() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_55.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_55.txt"
    );
}

#[test]
fn test_sample_56() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_56.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_56.txt"
    );
}

#[test]
fn test_sample_57() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_57.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_57.txt"
    );
}

#[test]
fn test_sample_58() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_58.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_58.txt"
    );
}

#[test]
fn test_sample_59() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_59.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_59.txt"
    );
}

#[test]
fn test_sample_60() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_60.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_60.txt"
    );
}

#[test]
fn test_sample_61() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_61.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_61.txt"
    );
}

#[test]
fn test_sample_62() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_62.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_62.txt"
    );
}

#[test]
fn test_sample_63() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_63.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_63.txt"
    );
}

#[test]
fn test_sample_64() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_64.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_64.txt"
    );
}

#[test]
fn test_sample_65() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_65.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_65.txt"
    );
}

#[test]
fn test_sample_66() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_66.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_66.txt"
    );
}

#[test]
fn test_sample_67() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_67.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_67.txt"
    );
}

#[test]
fn test_sample_68() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_68.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_68.txt"
    );
}

#[test]
fn test_sample_69() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_69.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_69.txt"
    );
}

#[test]
fn test_sample_70() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_70.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_70.txt"
    );
}

#[test]
fn test_sample_71() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_71.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_71.txt"
    );
}

#[test]
fn test_sample_72() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_72.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_72.txt"
    );
}

#[test]
fn test_sample_73() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_73.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_73.txt"
    );
}

#[test]
fn test_sample_74() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_74.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_74.txt"
    );
}

#[test]
fn test_sample_75() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_75.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_75.txt"
    );
}

#[test]
fn test_sample_76() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_76.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_76.txt"
    );
}

#[test]
fn test_sample_77() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_77.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_77.txt"
    );
}

#[test]
fn test_sample_78() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_78.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_78.txt"
    );
}

#[test]
fn test_sample_79() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_79.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_79.txt"
    );
}

#[test]
fn test_sample_80() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_80.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_80.txt"
    );
}

#[test]
fn test_sample_81() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_81.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_81.txt"
    );
}

#[test]
fn test_sample_82() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_82.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_82.txt"
    );
}

#[test]
fn test_sample_83() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_83.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_83.txt"
    );
}

#[test]
fn test_sample_84() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_84.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_84.txt"
    );
}

#[test]
fn test_sample_85() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_85.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_85.txt"
    );
}

#[test]
fn test_sample_86() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_86.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_86.txt"
    );
}

#[test]
fn test_sample_87() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_87.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_87.txt"
    );
}

#[test]
fn test_sample_88() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_88.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_88.txt"
    );
}

#[test]
fn test_sample_89() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_89.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_89.txt"
    );
}

#[test]
fn test_sample_90() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_90.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_90.txt"
    );
}

#[test]
fn test_sample_91() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_91.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_91.txt"
    );
}

#[test]
fn test_sample_92() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_92.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_92.txt"
    );
}

#[test]
fn test_sample_93() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_93.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_93.txt"
    );
}

#[test]
fn test_sample_94() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_94.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_94.txt"
    );
}

#[test]
fn test_sample_95() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_95.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_95.txt"
    );
}

#[test]
fn test_sample_96() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_96.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_96.txt"
    );
}

#[test]
fn test_sample_97() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_97.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_97.txt"
    );
}

#[test]
fn test_sample_98() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_98.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_98.txt"
    );
}

#[test]
fn test_sample_99() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("sample_99.txt"), "sample").unwrap();
    let walker = Walker::new().add_root(dir.path());
    let files: Vec<_> = walker
        .walk()
        .unwrap()
        .filter_map(walkkit::WalkItem::into_file)
        .collect();
    assert_eq!(files.len(), 1);
    assert_eq!(
        files[0]
            .path
            .file_name()
            .unwrap_or_else(|| std::process::exit(1)),
        "sample_99.txt"
    );
}
