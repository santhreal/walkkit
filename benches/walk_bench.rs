use criterion::{criterion_group, criterion_main, Criterion};
use walkkit::Walker;

fn bench_walk_tempdir(c: &mut Criterion) {
    let Ok(dir) = tempfile::tempdir() else {
        return;
    };
    for i in 0..100 {
        if std::fs::write(dir.path().join(format!("f{i}.txt")), format!("content {i}")).is_err() {
            return;
        }
    }
    for i in 0..10 {
        let sub = dir.path().join(format!("sub{i}"));
        if std::fs::create_dir(&sub).is_err() {
            return;
        }
        for j in 0..10 {
            if std::fs::write(sub.join(format!("f{j}.txt")), format!("nested {j}")).is_err() {
                return;
            }
        }
    }
    c.bench_function("walk_200_files", |b| {
        b.iter(|| {
            let Ok(iter) = Walker::new().add_root(dir.path()).walk() else {
                return 0;
            };
            let files: Vec<_> = iter.collect();
            files.len()
        });
    });
}

criterion_group!(benches, bench_walk_tempdir);
criterion_main!(benches);
