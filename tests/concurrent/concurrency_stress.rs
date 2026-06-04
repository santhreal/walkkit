use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::TempDir;
use walkkit::{SortMode, Walker};

#[test]
fn concurrency_stress_modification_while_walking() {
    let dir = TempDir::new().unwrap();
    let root = dir.path().to_path_buf();

    // Initial setup
    for i in 0..10 {
        let subdir = root.join(format!("initial_{}", i));
        fs::create_dir_all(&subdir).unwrap();
        fs::write(subdir.join("file.txt"), b"data").unwrap();
    }

    let num_threads = 10;
    let barrier = Arc::new(Barrier::new(num_threads + 1));
    let mut handles = vec![];

    for i in 0..num_threads {
        let b = barrier.clone();
        let r = root.clone();

        handles.push(thread::spawn(move || {
            b.wait();

            // Do some file modifications
            for j in 0..50 {
                let subdir = r.join(format!("thread_{}_{}", i, j));
                let _ = fs::create_dir_all(&subdir);
                let _ = fs::write(subdir.join("file.txt"), b"data");

                // Then try deleting some
                if j % 2 == 0 {
                    let _ = fs::remove_file(subdir.join("file.txt"));
                    let _ = fs::remove_dir(&subdir);
                }
            }
        }));
    }

    // Main thread acts as the walker
    barrier.wait();

    // Run walk continuously while other threads modify
    for _ in 0..5 {
        let files: Vec<_> = Walker::new()
            .add_root(&root)
            .with_parallelism(8)
            .walk()
            .unwrap()
            .collect();

        assert!(!files.is_empty(), "Should always find some files");
    }

    for h in handles {
        h.join().unwrap();
    }

    // Final run
    let final_files: Vec<_> = Walker::new()
        .add_root(&root)
        .with_parallelism(8)
        .with_sort(SortMode::ByName)
        .walk()
        .unwrap()
        .collect();

    assert!(!final_files.is_empty());
}
