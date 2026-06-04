#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Create a temp dir, populate with fuzz-derived structure, walk it
    if data.len() < 4 { return; }
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    // Create files based on fuzz data
    for (i, chunk) in data.chunks(4).enumerate().take(10) {
        let name = format!("f{i}.dat");
        let _ = std::fs::write(root.join(&name), chunk);
    }
    // Walk must not panic
    let _: Vec<_> = walkkit::Walker::new()
        .add_root(root)
        .with_parallelism(1)
        .walk()
        .collect();
});
