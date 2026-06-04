use proptest::prelude::*;
use std::fs;
use tempfile::TempDir;
use walkkit::{FileFilter, SortMode, Walker};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn prop_sorting_by_size_is_always_ascending(
        sizes in prop::collection::vec(0..10_000u64, 1..20)
    ) {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        for (i, size) in sizes.iter().enumerate() {
            fs::write(root.join(format!("file_{i}.txt")), vec![b'x'; *size as usize]).unwrap();
        }

        let files: Vec<_> = Walker::new()
            .add_root(root)
            .with_sort(SortMode::BySize)
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .collect();

        assert_eq!(files.len(), sizes.len());

        for i in 0..files.len() - 1 {
            assert!(files[i].size <= files[i+1].size);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn prop_extension_filter_consistency(
        ext_target in "([a-z0-9]{1,4})",
        filenames in prop::collection::vec("[a-z]{1,5}\\.([a-z0-9]{1,4})", 1..20)
    ) {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let mut expected_matches = 0;
        for (i, name) in filenames.iter().enumerate() {
            let path = root.join(format!("{i}_{name}"));
            fs::write(&path, b"data").unwrap();

            if let Some(ext) = std::path::Path::new(&name).extension() {
                if ext.to_string_lossy() == ext_target {
                    expected_matches += 1;
                }
            }
        }

        let files: Vec<_> = Walker::new()
            .add_root(root)
            .with_extension_filter(&ext_target)
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .collect();

        assert_eq!(files.len(), expected_matches);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn prop_size_limit_filtering(
        size_limit in 0..10_000u64,
        file_sizes in prop::collection::vec(0..15_000u64, 1..50)
    ) {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let mut expected_matches = 0;
        for (i, size) in file_sizes.iter().enumerate() {
            fs::write(root.join(format!("file_{i}.dat")), vec![b'x'; *size as usize]).unwrap();
            if *size <= size_limit {
                expected_matches += 1;
            }
        }

        let files: Vec<_> = Walker::new()
            .add_root(root)
            .with_size_limit(size_limit)
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .collect();

        assert_eq!(files.len(), expected_matches);
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_max_depth_filtering(
        max_depth in 0..10usize,
        paths in prop::collection::vec(prop::collection::vec("[a-z]{1,3}", 1..15), 1..20)
    ) {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        for path_parts in &paths {
            let mut current_path = root.to_path_buf();

            for part in &path_parts[0..path_parts.len()-1] {
                current_path = current_path.join(part);
                let _ = fs::create_dir_all(&current_path);
            }

            let file_path = current_path.join(&path_parts[path_parts.len()-1]);
            let _ = fs::write(&file_path, b"data");
        }

        let files: Vec<_> = Walker::new()
            .add_root(root)
            .with_max_depth(max_depth)
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .collect();

        for file in &files {
            let rel_path = file.path.strip_prefix(root).unwrap();
            let depth = rel_path.components().count().saturating_sub(1);
            assert!(depth <= max_depth, "Found file at depth {}, expected max {}", depth, max_depth);
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn prop_glob_filter_consistency(
        include_patterns in prop::collection::vec("(\\*\\*)?(/[a-z]{1,3})*(\\.[a-z]{1,3})?", 1..5),
        exclude_patterns in prop::collection::vec("(\\*\\*)?(/[a-z]{1,3})*(\\.[a-z]{1,3})?", 1..5),
        paths in prop::collection::vec("[a-z]{1,3}(/[a-z]{1,3})*(\\.[a-z]{1,3})?", 1..20)
    ) {
        let dir = TempDir::new().unwrap();
        let root = dir.path();

        let mut filter = FileFilter::new();
        for p in &include_patterns {
            filter = filter.add_include(p);
        }
        for p in &exclude_patterns {
            filter = filter.add_exclude(p);
        }

        let compiled = filter.compile();
        if compiled.is_err() { return Ok(()); } // Skip invalid random globs

        for path in &paths {
            let full_path = root.join(path);
            if let Some(parent) = full_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::write(&full_path, b"data");
        }

        let files: Vec<_> = Walker::new()
            .add_root(root)
            .with_filter(filter)
            .walk()
            .unwrap()
            .filter_map(walkkit::WalkItem::into_file)
            .collect();

        // Just verify it doesn't crash or hang, actual correctness of filtering
        // is delegated to globset, we just test the integration.
        assert!(files.len() <= paths.len());
    }
}
