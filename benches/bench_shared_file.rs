//! Misura accessi al conto su file: lettura e sostituzione con sincronizzazione.
use app_atm::BAccount;
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

fn benchmark(c: &mut Criterion) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("shared.txt");
    BAccount::initialize(&path, 1000).unwrap();
    c.bench_function("lettura_saldo_file", |b| {
        b.iter(|| BAccount::read(black_box(&path)).unwrap())
    });
    c.bench_function("scrittura_saldo_file_sync_rename", |b| {
        b.iter(|| BAccount::write(black_box(&path), black_box(400)).unwrap())
    });
}
criterion_group!(benches, benchmark);
criterion_main!(benches);
