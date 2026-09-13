//! Consegna locale dei log su tre porte distinte, indipendente dal canale del token.
use app_atm::BUdpLog;
use serde_json::{Value, json};
use std::{net::UdpSocket, time::Duration};

/// Prenota quattro porte UDP senza interferire con i blocchi di altre prove.
fn endpoints() -> (u16, Vec<UdpSocket>) {
    for attempt in 0..1000 {
        let base = 20000 + ((std::process::id() + attempt) % 9000) as u16 * 4;
        let sockets: Result<Vec<_>, _> = (base..base + 4)
            .map(|port| UdpSocket::bind(("127.0.0.1", port)))
            .collect();
        if let Ok(sockets) = sockets {
            return (base, sockets);
        }
    }
    panic!("Nessun blocco UDP libero");
}

/// Ogni console remota riceve una copia con la porta e l'identità del mittente.
#[test]
fn diffusione_udp_alle_tre_console_remote() {
    let (base, mut sockets) = endpoints();
    drop(sockets.remove(0));
    let log = BUdpLog::bind(1, base, true, None).unwrap();
    let record = json!({"atm":"ATM1","event":"token_created","hop":0,"balance":1000});
    log.publish(&record).unwrap();
    for receiver in sockets {
        receiver
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut data = [0u8; 8192];
        let (size, peer) = receiver.recv_from(&mut data).unwrap();
        assert_eq!(peer.port(), base);
        assert_eq!(
            serde_json::from_slice::<Value>(&data[..size]).unwrap(),
            record
        );
    }
    // L'assenza delle console non richiede conferme e non crea token alternativi.
    log.publish(&record).unwrap();
    assert!(log.publish(&json!({"message":"x".repeat(8193)})).is_err());
}

/// Il canale rifiuta configurazioni e bind incompatibili prima dell'avvio.
#[test]
fn configurazione_udp_invalida_e_porta_occupata_rifiutate() {
    let (base, _sockets) = endpoints();
    assert!(BUdpLog::bind(0, base, false, None).is_err());
    assert!(BUdpLog::bind(1, 65535, false, None).is_err());
    assert!(BUdpLog::bind(1, base, false, None).is_err());
}
