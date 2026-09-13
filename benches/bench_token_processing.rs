//! Misure dell’elaborazione di una visita, con preparazione esclusa dal tempo misurato.

use app_atm::{CAtm, CNode, CToken, CTransaction};
use criterion::{BatchSize, Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// Registra i casi misurati; black_box impedisce la rimozione del lavoro osservato.
fn benches(c: &mut Criterion) {
    c.bench_function("token/transaction", |b| {
        b.iter_batched(
            || {
                (
                    CNode::new(
                        CAtm::build("ATM2").unwrap(),
                        [CTransaction::withdrawal(200).unwrap()],
                    ),
                    CToken::create_initial(1000),
                )
            },
            |(mut node, token)| black_box(node.process_token(black_box(token)).unwrap()),
            BatchSize::SmallInput,
        )
    });
    c.bench_function("token/idle", |b| {
        b.iter_batched(
            || {
                (
                    CNode::new(CAtm::build("ATM1").unwrap(), []),
                    CToken::create_initial(1000),
                )
            },
            |(mut node, token)| black_box(node.process_token(black_box(token)).unwrap()),
            BatchSize::SmallInput,
        )
    });
}
criterion_group!(bench_token_processinges, benches);
criterion_main!(bench_token_processinges);
