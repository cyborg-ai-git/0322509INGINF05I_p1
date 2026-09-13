//! Misure separate di codifica JSON e decodifica con validazione di dominio.

use app_atm::{CAtm, CMessage, CToken};
use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;
/// Registra i casi misurati; black_box impedisce la rimozione del lavoro osservato.
fn benches(c: &mut Criterion) {
    let mut token = CToken::create_initial(1000);
    CToken::mark_processed_by(&mut token, &CAtm::build("ATM1").unwrap()).unwrap();
    let encoded = CMessage::encode_token(&token).unwrap();
    c.bench_function("codec/encode", |b| {
        b.iter(|| CMessage::encode_token(black_box(&token)).unwrap())
    });
    c.bench_function("codec/decode_validate", |b| {
        b.iter(|| CMessage::decode_token(black_box(&encoded)).unwrap())
    });
}
criterion_group!(bench_message_codeces, benches);
criterion_main!(bench_message_codeces);
