//! Misura di un passaggio TCP con listener effimero, non di un anello completo.

use app_atm::{CMessage, CToken};
use criterion::{BatchSize, Criterion, SamplingMode, criterion_group, criterion_main};
use std::{hint::black_box, thread, time::Duration};

/// Misura un trasferimento isolato, distanziando i campioni fuori dal tempo misurato.
fn benches(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let token = CToken::create_initial(1000);
    let mut group = c.benchmark_group("tcp");
    group.sampling_mode(SamplingMode::Flat);
    // Ogni passaggio crea una connessione nuova. La pausa di preparazione limita
    // il consumo delle porte effimere mentre le precedenti sono in TIME_WAIT.
    // PerIteration esclude la pausa dalla misura; Criterion la considera nella
    // calibrazione del numero di iterazioni. Non modifica il protocollo degli ATM.
    // Si misura la latenza di passaggi isolati, non il massimo throughput di rete.
    group.bench_function("one_hop_with_bind", |b| {
        b.iter_batched(
            || thread::sleep(Duration::from_millis(5)),
            |()| {
                rt.block_on(async {
                    tokio::time::timeout(Duration::from_secs(3), async {
                        let listener = CMessage::bind_listener("127.0.0.1:0".parse().unwrap())
                            .await
                            .unwrap();
                        let address = listener.local_addr().unwrap();
                        let (sent, received) = tokio::join!(
                            CMessage::send_token_once(address, black_box(&token)),
                            async {
                                let (stream, _) = listener.accept().await.unwrap();
                                CMessage::read_token(stream).await.unwrap()
                            }
                        );
                        sent.unwrap();
                        black_box(received)
                    })
                    .await
                    .expect("Passaggio TCP non completato entro tre secondi")
                })
            },
            BatchSize::PerIteration,
        )
    });

    // Prepara una sola connessione, come nell’anello in esecuzione. Bind e
    // apertura iniziale restano fuori dalla misura dei successivi trasferimenti.
    let mut outgoing = None;
    let mut incoming = rt.block_on(async {
        let listener = CMessage::bind_listener("127.0.0.1:0".parse().unwrap())
            .await
            .unwrap();
        let address = listener.local_addr().unwrap();
        let (sent, accepted) = tokio::join!(
            CMessage::send_token_reusing_connection(
                &mut outgoing,
                address,
                &token,
                Duration::from_millis(1)
            ),
            listener.accept()
        );
        sent.unwrap();
        let mut reader = tokio::io::BufReader::new(accepted.unwrap().0);
        CMessage::read_next_token(&mut reader)
            .await
            .unwrap()
            .unwrap();
        reader
    });
    let successor = outgoing.as_ref().unwrap().peer_addr().unwrap();
    group.bench_function("one_hop_reused_connection", |b| {
        b.iter(|| {
            rt.block_on(async {
                tokio::time::timeout(Duration::from_secs(3), async {
                    let (sent, received) = tokio::join!(
                        CMessage::send_token_reusing_connection(
                            &mut outgoing,
                            successor,
                            black_box(&token),
                            Duration::from_millis(1)
                        ),
                        CMessage::read_next_token(&mut incoming)
                    );
                    sent.unwrap();
                    black_box(received.unwrap().expect("Connessione aperta"))
                })
                .await
                .expect("Passaggio TCP persistente entro tre secondi")
            })
        })
    });
    group.finish();
}
criterion_group!(bench_tcpes, benches);
criterion_main!(bench_tcpes);
