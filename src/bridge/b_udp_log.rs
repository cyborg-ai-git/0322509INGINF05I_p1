//! Diffusione applicativa dei log: copie UDP su quattro porte distinte di localhost.

use crate::CAtm;
use serde_json::{Value, json};
use std::{
    io::{self, Write},
    net::{Ipv4Addr, SocketAddrV4, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

/// Canale di osservazione dei log, indipendente dal trasporto TCP del token.
///
/// Il mittente invia una copia a ciascuno degli altri tre ATM; la propria console
/// scrive già il log locale. Si tratta di diffusione applicativa tramite unicast,
/// non del broadcast IP di rete. Nessun datagramma concede il permesso di operare.
pub struct BUdpLog {
    socket: UdpSocket,
    destinations: Vec<SocketAddrV4>,
    stop: Arc<AtomicBool>,
    reader: Option<JoinHandle<()>>,
}

impl BUdpLog {
    /// Apre la porta UDP del nodo e avvia il solo lettore dei log remoti.
    ///
    /// `node` è compreso fra 1 e 4; le porte sono `base..=base+3`. La conoscenza
    /// di queste porte riguarda soltanto l'osservazione: il token va al successore.
    /// Il limite di hop agisce sulla vista, senza rallentare i nodi inattivi.
    ///
    /// # Errori
    /// Rifiuta identificativi, porte non valide e porte occupate. Propaga errori
    /// di creazione del socket e del thread; non altera lo stato del conto.
    pub fn bind(node: u16, base: u16, json_output: bool, limit: Option<u64>) -> io::Result<Self> {
        if !(1..=4).contains(&node) || !(1024..=65532).contains(&base) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "nodo o porte UDP non validi",
            ));
        }
        let socket = UdpSocket::bind((Ipv4Addr::LOCALHOST, base + node - 1))?;
        socket.set_read_timeout(Some(Duration::from_millis(100)))?;
        let receiver = socket.try_clone()?;
        let stop = Arc::new(AtomicBool::new(false));
        let quitting = stop.clone();
        let reader = thread::Builder::new()
            .name(format!("log-udp-atm{node}"))
            .spawn(move || {
                let mut buffer = [0u8; 8192];
                while !quitting.load(Ordering::Relaxed) {
                    let (size, peer) = match receiver.recv_from(&mut buffer) {
                        Ok(value) => value,
                        Err(e)
                            if matches!(
                                e.kind(),
                                io::ErrorKind::WouldBlock
                                    | io::ErrorKind::TimedOut
                                    | io::ErrorKind::Interrupted
                            ) =>
                        {
                            continue;
                        }
                        Err(_) => break,
                    };
                    let Ok(record) = serde_json::from_slice::<Value>(&buffer[..size]) else {
                        continue;
                    };
                    let Some(atm) = record["atm"].as_str() else {
                        continue;
                    };
                    let Ok(identity) = CAtm::build(atm) else {
                        continue;
                    };
                    let Ok(index) = CAtm::index(&identity) else {
                        continue;
                    };
                    // Scarta datagrammi estranei o riflessi; non è autenticazione crittografica.
                    if !peer.ip().is_loopback() || peer.port() != base + index || index + 1 == node
                    {
                        continue;
                    }
                    if limit
                        .is_some_and(|last| record["hop"].as_u64().is_some_and(|hop| hop > last))
                    {
                        continue;
                    }
                    let stdout = io::stdout();
                    let mut output = stdout.lock();
                    let result = if json_output {
                        writeln!(output, "{}", json!({"event":"remote_log","source":record}))
                    } else {
                        writeln!(
                            output,
                            "[UDP][{atm}] {} | hop={} | saldo={}",
                            super::b_log::event_label(record["event"].as_str().unwrap_or("evento")),
                            record["hop"],
                            record["balance"]
                        )
                    };
                    if result.and_then(|()| output.flush()).is_err() {
                        break;
                    }
                }
            })?;
        let destinations = (1..=4)
            .filter(|id| *id != node)
            .map(|id| SocketAddrV4::new(Ipv4Addr::LOCALHOST, base + id - 1))
            .collect();
        Ok(Self {
            socket,
            destinations,
            stop,
            reader: Some(reader),
        })
    }

    /// Invia copie del record alle tre console remote senza attendere conferme.
    ///
    /// # Errori
    /// Propaga errori di invio e rifiuta record oltre 8192 byte. UDP può perdere
    /// o riordinare messaggi: il log locale rimane la fonte delle verifiche causali.
    pub fn publish(&self, record: &Value) -> io::Result<()> {
        let bytes = serde_json::to_vec(record)?;
        if bytes.len() > 8192 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "log UDP troppo lungo",
            ));
        }
        for destination in &self.destinations {
            self.socket.send_to(&bytes, destination)?;
        }
        Ok(())
    }
}

impl Drop for BUdpLog {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
