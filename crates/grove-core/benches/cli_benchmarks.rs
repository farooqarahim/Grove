use criterion::{Criterion, criterion_group, criterion_main};
use tempfile::TempDir;

fn bench_db_initialize(c: &mut Criterion) {
    c.bench_function("db_initialize", |b| {
        b.iter_with_setup(
            || TempDir::new().unwrap(),
            |dir| grove_core::db::initialize(dir.path()).unwrap(),
        )
    });
}

fn bench_config_load(c: &mut Criterion) {
    let dir = TempDir::new().unwrap();
    grove_core::db::initialize(dir.path()).unwrap();

    c.bench_function("config_load", |b| {
        b.iter(|| grove_core::config::GroveConfig::load_or_create(dir.path()).unwrap())
    });
}

fn bench_list_runs_empty(c: &mut Criterion) {
    let dir = TempDir::new().unwrap();
    grove_core::db::initialize(dir.path()).unwrap();

    c.bench_function("list_runs_empty", |b| {
        b.iter(|| grove_core::orchestrator::list_runs(dir.path(), 100).unwrap())
    });
}

criterion_group!(
    benches,
    bench_db_initialize,
    bench_config_load,
    bench_list_runs_empty
);
criterion_main!(benches);
