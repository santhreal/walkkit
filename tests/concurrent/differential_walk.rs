use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Write;
use tempfile::tempdir;
use walkkit::{WalkItem, Walker};

#[test]
fn test_differential_single_vs_multi_thread_equivalence() {
    let dir = tempdir().expect("create temp dir");
    let root = dir.path();

    // Create a complex directory structure
    let sub_a = root.join("dir_a");
    let sub_b = sub_a.join("dir_b");
    let sub_c = sub_b.join("dir_c");
    fs::create_dir_all(&sub_c).expect("create nested dirs");

    let sub_x = root.join("dir_x");
    let sub_y = sub_x.join("dir_y");
    fs::create_dir_all(&sub_y).expect("create dir_y");

    let ignored_dir = root.join("ignored_dir");
    fs::create_dir_all(&ignored_dir).expect("create ignored_dir");

    // Files
    fs::write(root.join("root_file.txt"), "hello root").unwrap();
    fs::write(sub_a.join("a_file.rs"), "fn main() {}").unwrap();
    fs::write(sub_b.join("b_file.rs"), "pub fn b() {}").unwrap();
    fs::write(sub_c.join("c_file.rs"), "pub fn c() {}").unwrap();
    fs::write(sub_x.join("x_file.md"), "# Heading").unwrap();
    fs::write(sub_y.join("y_file.json"), "{}").unwrap();
    fs::write(ignored_dir.join("should_be_ignored.txt"), "secret").unwrap();
    fs::write(root.join("app.log"), "log line 1\nlog line 2").unwrap();

    // Binary file
    let mut bin_file = File::create(sub_a.join("data.bin")).unwrap();
    bin_file.write_all(b"HEADER\x00BINARY_DATA\x00END").unwrap();

    // Gitignore file
    fs::write(root.join(".gitignore"), "ignored_dir/\n*.log\n").unwrap();

    // Collect baseline with parallelism = 1 (single-threaded)
    let get_walk_results = |p: usize| {
        let items: Vec<WalkItem> = Walker::new()
            .add_root(root)
            .with_parallelism(p)
            .respect_gitignore(true)
            .walk()
            .expect("walk succeeds")
            .collect();

        let mut files = BTreeSet::new();
        let mut errors = BTreeSet::new();

        for item in items {
            match item {
                WalkItem::File(f) => {
                    let rel_path = f
                        .path
                        .strip_prefix(root)
                        .unwrap_or(&f.path)
                        .to_string_lossy()
                        .to_string();
                    files.insert((rel_path, f.size));
                }
                WalkItem::Error(e) => {
                    let rel_path = e
                        .path
                        .strip_prefix(root)
                        .unwrap_or(&e.path)
                        .to_string_lossy()
                        .to_string();
                    errors.insert((rel_path, format!("{:?}", e.op)));
                }
            }
        }

        (files, errors)
    };

    let (baseline_files, baseline_errors) = get_walk_results(1);

    // Verify baseline found expected files
    assert!(
        baseline_files
            .iter()
            .any(|(p, _)| p == "root_file.txt"),
        "baseline must include root_file.txt"
    );
    assert!(
        !baseline_files
            .iter()
            .any(|(p, _)| p.contains("ignored_dir")),
        "ignored_dir must be excluded"
    );
    assert!(
        !baseline_files
            .iter()
            .any(|(p, _)| p.ends_with(".log")),
        "*.log must be excluded"
    );

    // Assert equivalence across parallelism levels 2, 4, 8
    for p in [2, 4, 8] {
        let (files, errors) = get_walk_results(p);
        assert_eq!(
            files, baseline_files,
            "file set mismatch between parallelism=1 and parallelism={p}"
        );
        assert_eq!(
            errors, baseline_errors,
            "error set mismatch between parallelism=1 and parallelism={p}"
        );
    }
}
