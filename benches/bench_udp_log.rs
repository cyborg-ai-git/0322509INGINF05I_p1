//! Misura il solo invio di tre copie UDP; non include ricezione o rendering dei log.
use app_atm::BUdpLog;
use criterion::{Criterion, criterion_group, criterion_main};
use serde_json::json;
use std::{hint::black_box, net::UdpSocket};

fn benchmark(c: &mut Criterion) {
    // Prenota quattro porte: non assume che le porte della dimostrazione siano libere.
    let (base, mut sockets) = (0..1000)
        .find_map(|attempt| {
            let base = 20000 + ((std::process::id() + attempt) % 9000) as u16 * 4;
            let sockets: Result<Vec<_>, _> = (base..base + 4)
                .map(|port| UdpSocket::bind(("127.0.0.1", port)))
                .collect();
            sockets.ok().map(|s| (base, s))
        })
        .expect("Quattro porte UDP libere");
    drop(sockets.remove(0));
    let log = BUdpLog::bind(1, base, true, None).unwrap();
    let record = json!({"atm":"ATM1", "event":"token_received", "hop":4, "balance":400});
    // Le destinazioni restano aperte; UDP può scartare datagrammi quando i buffer sono pieni.
    c.bench_function("invio_log_udp_tre_destinatari", |b| {
        b.iter(|| log.publish(black_box(&record)).unwrap())
    });
    drop(sockets);
}
criterion_group!(benches, benchmark);
criterion_main!(benches);
