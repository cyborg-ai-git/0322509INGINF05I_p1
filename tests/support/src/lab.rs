//! Supervisore esterno: avvia e osserva i processi senza inviare messaggi bancari.
use crate::{Result, random_id, terminal::Session, verify};
use anyhow::{Context, ensure};
use portable_pty::CommandBuilder;
use serde_json::{Value, json};
use socket2::{Domain, Protocol, Socket, Type};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// Quattro sessioni indipendenti e risultato della sola osservazione dei log.
pub struct Lab {
    /// Cartella dei log di questa esecuzione, mai condivisa con un altro avvio.
    pub log_dir: PathBuf,
    /// Un oggetto PTY distinto per ogni ATM.
    pub sessions: Vec<Session>,
    result: Mutex<Option<std::result::Result<Value, String>>>,
    quit: AtomicBool,
}

impl Lab {
    /// Avvia un binario esplicito, utile ai test Cargo senza una build release precedente.
    pub fn start_with_binary(root: &Path, binary: &Path, delay: u64) -> Result<Arc<Self>> {
        let parent = root.join("demo_logs");
        fs::create_dir_all(&parent)?;
        let log_dir = parent.join(format!("run_{}", random_id()?));
        fs::create_dir(&log_dir)?;
        let (base, mut reservations, mut udp_reservations) = reserve_ports()?;
        ensure!(
            binary.is_file(),
            "Binario ATM richiesto dalla prova non trovato"
        );
        let mut sessions = Vec::new();
        let transactions = [
            None,
            Some("withdraw:200"),
            Some("deposit:100"),
            Some("withdraw:500"),
        ];
        for node in 0..4usize {
            let mut command = CommandBuilder::new(binary);
            command.cwd(root);
            command.args([
                "--id",
                &format!("ATM{}", node + 1),
                "--bind",
                &format!("127.0.0.1:{}", base + node as u16),
                "--successor",
                &format!("127.0.0.1:{}", base + (node as u16 + 1) % 4),
                "--startup-delay-ms",
                "1000",
                "--transaction-delay-ms",
                &delay.to_string(),
                "--console-hop-limit",
                "11",
                "--retry-delay-ms",
                "50",
                "--audit-file",
            ]);
            command.arg(log_dir.join(format!("atm{}.jsonl", node + 1)));
            command.arg("--account-file");
            command.arg(log_dir.join("shared.txt"));
            command.args(["--log-base-port", &base.to_string()]);
            if node == 0 {
                command.arg("--initial-token");
            }
            if let Some(tx) = transactions[node] {
                command.args(["--transaction", tx]);
            }
            reservations[node].take();
            udp_reservations[node].take();
            sessions.push(Session::spawn(
                &format!("ATM{}", node + 1),
                command,
                &log_dir.join(format!("atm{}.terminal.log", node + 1)),
            )?);
        }
        let lab = Arc::new(Self {
            log_dir,
            sessions,
            result: Mutex::new(None),
            quit: AtomicBool::new(false),
        });
        let identities: Vec<_> = lab
            .sessions
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let mut value = s.identity();
                value["atm_port"] = json!(base + i as u16);
                value
            })
            .collect();
        fs::write(
            lab.log_dir.join("launch.json"),
            serde_json::to_vec_pretty(&identities)?,
        )?;
        Ok(lab)
    }

    /// Osserva un tratto finito, arresta tutti i figli e conserva i log della prova.
    pub fn monitor(&self) {
        let result = self
            .observe()
            .and_then(|()| self.stop_processes())
            .and_then(|()| verify::verify(&self.log_dir, 12, true));
        // Anche un errore di lettura o una chiusura anticipata deve liberare i figli.
        if result.is_err() {
            let _ = self.stop_processes();
        }
        if let Ok(value) = &result
            && let Err(error) = fs::write(
                self.log_dir.join("verification.json"),
                serde_json::to_vec_pretty(value).expect("Valore JSON"),
            )
        {
            *self.result.lock().unwrap_or_else(|e| e.into_inner()) = Some(Err(error.to_string()));
            return;
        }
        *self.result.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(result.map_err(|e| e.to_string()));
    }

    fn observe(&self) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(45);
        loop {
            ensure!(
                !self.quitting(),
                "Dimostrazione interrotta prima della verifica"
            );
            ensure!(
                Instant::now() < deadline,
                "L'anello non ha completato quattro giri"
            );
            // Un prefisso corretto non basta se un nodo è già terminato: controlla
            // prima i processi, poi le evidenze raccolte durante la loro esecuzione.
            for session in &self.sessions {
                ensure!(
                    session.running()?,
                    "ATM terminato prima della verifica: {}",
                    session.identity()
                );
            }
            let file = self.log_dir.join("atm1.jsonl");
            if file.exists() {
                let raw = fs::read(file)?;
                for line in raw
                    .split_inclusive(|b| *b == b'\n')
                    .filter(|l| l.ends_with(b"\n"))
                {
                    let event: Value = serde_json::from_slice(line)?;
                    if event["event"] == "token_received"
                        && event["hop"].as_u64().is_some_and(|h| h >= 16)
                    {
                        return Ok(());
                    }
                }
            }
            thread::sleep(Duration::from_millis(30));
        }
    }

    /// Scrive il marcatore prima dell'arresto; tenta la chiusura di tutti i figli.
    pub fn stop_processes(&self) -> Result<()> {
        let marker = self.log_dir.join("shutdown.json");
        if !marker.exists() {
            fs::write(
                marker,
                serde_json::to_vec_pretty(
                    &json!({"requested_unix_ns":SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos().to_string(),
                "reason":"Arresto esterno della dimostrazione", "pids":self.sessions.iter().map(Session::pid).collect::<Vec<_>>()}),
                )?,
            )?;
        }
        let mut first = None;
        for session in &self.sessions {
            if let Err(error) = session.stop() {
                first.get_or_insert(error);
            }
        }
        if let Some(error) = first {
            return Err(error);
        }
        Ok(())
    }

    /// Stato dell'osservatore, leggibile senza consumare gli output delle console.
    pub fn status(&self) -> Value {
        let result = self.result.lock().unwrap_or_else(|e| e.into_inner());
        let (status, verified, error) = match &*result {
            None => ("running", Value::Null, Value::Null),
            Some(Ok(value)) => ("verified", value.clone(), Value::Null),
            Some(Err(message)) => ("failed", Value::Null, json!(message)),
        };
        json!({"status":status,"result":verified,"error":error,"log_dir":self.log_dir,
            "sessions":self.sessions.iter().map(Session::identity).collect::<Vec<_>>()})
    }

    /// Chiede la chiusura dei soli processi appartenenti a questo laboratorio.
    pub fn request_stop(&self) {
        self.quit.store(true, Ordering::SeqCst);
    }

    /// Indica se è stata richiesta la chiusura del laboratorio.
    pub fn quitting(&self) -> bool {
        self.quit.load(Ordering::SeqCst)
    }
}

type Reservations = (u16, Vec<Option<Socket>>, Vec<Option<Socket>>);

fn reserve_ports() -> Result<Reservations> {
    for index in 0..200 {
        let base = 20000 + ((std::process::id() + index) % 5000) as u16 * 4;
        let mut sockets = Vec::new();
        let mut udp = Vec::new();
        for port in base..base + 4 {
            let socket = Socket::new(Domain::IPV4, Type::STREAM, Some(Protocol::TCP))?;
            let address = format!("127.0.0.1:{port}").parse::<std::net::SocketAddr>()?;
            if socket.bind(&address.into()).is_err() {
                break;
            }
            let log_socket = Socket::new(Domain::IPV4, Type::DGRAM, Some(Protocol::UDP))?;
            if log_socket.bind(&address.into()).is_err() {
                break;
            }
            sockets.push(Some(socket));
            udp.push(Some(log_socket));
        }
        if sockets.len() == 4 {
            return Ok((base, sockets, udp));
        }
    }
    None.context("Nessun blocco di quattro porte libere")
}
