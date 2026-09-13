//! Misure di deposito, prelievo, fondi insufficienti e overflow.

use app_atm::{CAtm, CTransaction};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
/// Registra i casi misurati; black_box impedisce la rimozione del lavoro osservato.
fn benches(c: &mut Criterion) {
    let atm = CAtm::build("ATM2").unwrap();
    for (name, transaction, balance) in [
        (
            "transaction/deposit",
            CTransaction::deposit(100).unwrap(),
            1000,
        ),
        (
            "transaction/withdrawal",
            CTransaction::withdrawal(100).unwrap(),
            1000,
        ),
        (
            "transaction/rejected",
            CTransaction::withdrawal(2000).unwrap(),
            1000,
        ),
        (
            "transaction/overflow",
            CTransaction::deposit(1).unwrap(),
            i64::MAX,
        ),
    ] {
        c.bench_function(name, |b| {
            b.iter(|| {
                CTransaction::apply(
                    black_box(&atm),
                    black_box(balance),
                    black_box(transaction.clone()),
                )
            })
        });
    }
}
criterion_group!(bench_transactiones, benches);
criterion_main!(bench_transactiones);
