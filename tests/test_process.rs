//! Test esterni di processi reali, messaggi TCP e intervalli di sezione critica.
use serde_json::Value;
use std::sync::atomic::{AtomicU16, Ordering};
use std::{
    collections::{BTreeMap, HashSet},
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::net::TcpSocket;

static NEXT_BLOCK: AtomicU16 = AtomicU16::new(0);
// Porte volutamente non consecutive: la CLI riceve separatamente i due endpoint.
const PORT_OFFSETS: [u16; 4] = [0, 11, 3, 7];
// Serializza soltanto le diverse prove del supervisore, che prenotano porte
// e creano processi sullo stesso host. Dentro ogni prova i quattro ATM restano
// concorrenti e non ricevono né conoscono questo lock del processo di test.
static TEST_ENV: Mutex<()> = Mutex::new(());

struct Ring {
    _environment: std::sync::MutexGuard<'static, ()>,
    base: u16,
    root: tempfile::TempDir,
    udp_reservations: Vec<Option<std::net::UdpSocket>>,
    reservations: Vec<Option<TcpSocket>>,
    children: Vec<Child>,
    sender: Sender<Value>,
    receiver: Receiver<Value>,
    events: Vec<Value>,
    errors: Arc<Mutex<Vec<String>>>,
}
impl Ring {
    fn new() -> Self {
        let environment = TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let (sender, receiver) = mpsc::channel();
        for _ in 0..200 {
            // Evita il normale intervallo effimero di macOS e blocchi condivisi
            // fra test concorrenti. Il bind resta la verifica effettiva di disponibilità.
            let serial = u32::from(NEXT_BLOCK.fetch_add(1, Ordering::Relaxed));
            let base = 20000 + ((std::process::id() + serial) % 5000) as u16 * 4;
            let first = TcpSocket::new_v4().unwrap();
            if first
                .bind(format!("127.0.0.1:{base}").parse().unwrap())
                .is_err()
            {
                continue;
            }
            let mut reservations = vec![Some(first)];
            for offset in PORT_OFFSETS.iter().skip(1) {
                let socket = TcpSocket::new_v4().unwrap();
                if socket
                    .bind(format!("127.0.0.1:{}", base + offset).parse().unwrap())
                    .is_ok()
                {
                    reservations.push(Some(socket));
                } else {
                    break;
                }
            }
            let udp_reservations: Vec<_> = (base..base + 4)
                .map(|port| std::net::UdpSocket::bind((std::net::Ipv4Addr::LOCALHOST, port)).ok())
                .collect();
            if reservations.len() == 4 && udp_reservations.iter().all(Option::is_some) {
                return Self {
                    _environment: environment,
                    base,
                    root: tempfile::tempdir().unwrap(),
                    udp_reservations,
                    reservations,
                    children: vec![],
                    sender,
                    receiver,
                    events: vec![],
                    errors: Arc::new(Mutex::new(vec![])),
                };
            }
        }
        panic!("no free four-port block");
    }
    fn start(&mut self, index: usize, transactions: &[&str], cs_delay: u64) {
        self.reservations[index].take();
        self.udp_reservations[index].take();
        let mut command = Command::new(env!("CARGO_BIN_EXE_app_atm"));
        command.args([
            "--id",
            &(index + 1).to_string(),
            "--bind",
            &format!("127.0.0.1:{}", self.base + PORT_OFFSETS[index]),
            "--successor",
            &format!("127.0.0.1:{}", self.base + PORT_OFFSETS[(index + 1) % 4]),
            "--startup-delay-ms",
            "0",
            "--retry-delay-ms",
            "5",
            "--transaction-delay-ms",
            &cs_delay.to_string(),
            "--json",
        ]);
        command
            .arg("--account-file")
            .arg(self.root.path().join("shared.txt"));
        command.args(["--log-base-port", &self.base.to_string()]);
        if index == 0 {
            command.arg("--initial-token");
        }
        for transaction in transactions {
            command.args(["--transaction", transaction]);
        }
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let output = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let errors = self.errors.clone();
        thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                errors
                    .lock()
                    .unwrap()
                    .push(format!("ATM{}: {line}", index + 1));
            }
        });
        let sender = self.sender.clone();
        thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                let mut value: Value = serde_json::from_str(&line).expect("valid audit JSON");
                value["console"] = serde_json::json!(index + 1);
                if sender.send(value).is_err() {
                    break;
                }
            }
        });
        self.children.push(child);
    }
    fn until(&mut self, condition: impl Fn(&[Value]) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(8);
        while !condition(&self.events) {
            assert!(
                Instant::now() < deadline,
                "trace timeout; {} events",
                self.events.len()
            );
            if let Ok(event) = self.receiver.recv_timeout(Duration::from_millis(50)) {
                self.events.push(event);
            }
            for child in &mut self.children {
                if let Some(status) = child.try_wait().unwrap() {
                    thread::sleep(Duration::from_millis(10));
                    panic!(
                        "processo {} terminato con {status}; stderr: {:?}",
                        child.id(),
                        self.errors.lock().unwrap()
                    );
                }
            }
        }
    }
    fn count(&mut self, event: &str, n: usize) {
        self.until(|events| events.iter().filter(|e| e["event"] == event).count() >= n);
    }
}
impl Drop for Ring {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
        }
        for child in &mut self.children {
            let _ = child.wait();
        }
    }
}
fn time(event: &Value) -> u128 {
    event["unix_ns"].as_str().unwrap().parse().unwrap()
}

/// Attende tutte le coppie causali necessarie prima di valutarle, indipendentemente dai lettori.
fn prefix_complete(events: &[Value], last_hop: u64) -> bool {
    (0..=last_hop).all(|hop| {
        events
            .iter()
            .any(|e| e["event"] == "token_received" && e["hop"] == hop)
    }) && (1..=last_hop).all(|hop| {
        events
            .iter()
            .any(|e| e["event"] == "token_forwarding" && e["hop"] == hop)
    })
}

/// Verifica il prefisso finito: unicità delle visite, quattro PID e intervalli di CS disgiunti.
fn verify(events: &[Value], expected_transactions: usize, last_hop: u64) {
    let received: Vec<_> = events
        .iter()
        .filter(|e| e["event"] == "token_received" && e["hop"].as_u64().unwrap() <= last_hop)
        .collect();
    let mut hops = HashSet::new();
    let mut pids = HashSet::new();
    for event in &received {
        let hop = event["hop"].as_u64().unwrap();
        assert!(hops.insert(hop), "duplicate token receipt at hop {hop}");
        assert_eq!(event["atm"], format!("ATM{}", hop % 4 + 1));
        pids.insert(event["pid"].as_u64().unwrap());
        if hop > 0 {
            let sent = events
                .iter()
                .find(|e| e["event"] == "token_forwarding" && e["hop"] == hop)
                .expect("receipt has a causal send");
            assert_eq!(sent["balance"], event["balance"]);
            assert!(time(sent) <= time(event));
        }
    }
    assert_eq!(hops.len() as u64, last_hop + 1);
    assert_eq!(pids.len(), 4);
    assert_eq!(
        events
            .iter()
            .filter(|e| e["event"] == "token_created")
            .count(),
        1
    );
    let mut intervals = BTreeMap::new();
    for end in events
        .iter()
        .filter(|e| e["event"] == "transaction_finished")
    {
        let hop = end["hop"].as_u64().unwrap();
        let start = events
            .iter()
            .find(|e| e["event"] == "transaction_started" && e["hop"] == hop)
            .expect("matching start");
        assert_eq!(start["atm"], end["atm"]);
        assert_eq!(
            start["balance"],
            end["detail"]["outcome"]["previous_balance"]
        );
        assert!(time(start) <= time(end));
        assert!(intervals.insert(hop, (time(start), time(end))).is_none());
        if let Some(sent) = events
            .iter()
            .find(|e| e["event"] == "token_forwarding" && e["hop"] == hop + 1)
        {
            assert!(time(end) <= time(sent));
            assert_eq!(end["balance"], sent["balance"]);
        }
    }
    assert_eq!(intervals.len(), expected_transactions);
    let mut last_end = 0;
    for (start, end) in intervals.values() {
        assert!(*start >= last_end, "overlapping critical sections");
        last_end = *end;
    }
}

/// Verifica quattro PID, la sequenza 1000 → 800 → 900 → 400 e i giri inattivi.
#[test]
fn four_real_processes_reproduce_pdf_and_keep_circulating_without_idle_sleep() {
    let mut ring = Ring::new();
    for (i, tx) in [
        (3, vec!["withdraw:500"]),
        (2, vec!["deposit:100"]),
        (1, vec!["withdraw:200"]),
        (0, vec![]),
    ] {
        ring.start(i, &tx, 10);
    }
    ring.until(|es| prefix_complete(es, 12));
    verify(&ring.events, 3, 12);
    assert_eq!(
        std::fs::read_to_string(ring.root.path().join("shared.txt"))
            .unwrap()
            .trim(),
        "400"
    );
    for (hop, balance) in [
        (0, 1000),
        (1, 1000),
        (2, 800),
        (3, 900),
        (4, 400),
        (8, 400),
        (12, 400),
    ] {
        assert!(
            ring.events.iter().any(|e| e["event"] == "token_received"
                && e["hop"] == hop
                && e["balance"] == balance)
        );
    }
}

/// Verifica più code FIFO, un rifiuto senza fondi e il saldo risultante via TCP.
#[test]
fn queued_transactions_are_fair_and_rejections_preserve_balance_over_tcp() {
    let mut ring = Ring::new();
    for (i, tx) in [
        (0, vec!["deposit:1", "withdraw:1"]),
        (1, vec!["withdraw:2000", "deposit:2"]),
        (2, vec!["deposit:3", "withdraw:3"]),
        (3, vec!["withdraw:4", "deposit:4"]),
    ] {
        ring.start(i, &tx, 4);
    }
    ring.until(|es| prefix_complete(es, 12));
    verify(&ring.events, 8, 12);
    assert_eq!(
        std::fs::read_to_string(ring.root.path().join("shared.txt"))
            .unwrap()
            .trim(),
        "1002"
    );
    assert!(
        ring.events
            .iter()
            .any(|e| e["event"] == "transaction_finished"
                && e["hop"] == 1
                && e["balance"] == 1001
                && e["detail"]["outcome"]["status"].get("Rejected").is_some())
    );
    assert!(
        ring.events
            .iter()
            .any(|e| e["event"] == "token_received" && e["hop"] == 8 && e["balance"] == 1002)
    );
}

/// L’assenza di ATM2 impedisce le transazioni; il suo avvio riprende lo stesso token.
#[test]
fn missing_node_stalls_the_ring_and_late_start_resumes_the_single_token() {
    let mut ring = Ring::new();
    // La prenotazione esegue solo bind, senza listen: protegge la porta e
    // rifiuta le connessioni finché non viene avviato il vero processo ATM2.
    ring.start(0, &[], 0);
    ring.start(2, &["deposit:100"], 0);
    ring.start(3, &["withdraw:500"], 0);
    ring.count("node_ready", 3);
    thread::sleep(Duration::from_millis(60));
    while let Ok(event) = ring.receiver.try_recv() {
        ring.events.push(event);
    }
    assert!(
        !ring
            .events
            .iter()
            .any(|e| e["event"] == "transaction_started")
    );
    ring.start(1, &["withdraw:200"], 0);
    ring.until(|es| prefix_complete(es, 8));
    verify(&ring.events, 3, 8);
}

/// Un ATM isolato con richieste emette solo l’avvio e non entra in sezione critica.
#[test]
fn isolated_atm_never_updates_without_receiving_a_token() {
    let mut ring = Ring::new();
    std::fs::write(ring.root.path().join("shared.txt"), "1000\n").unwrap();
    ring.start(1, &["deposit:100"], 0);
    ring.count("node_ready", 1);
    thread::sleep(Duration::from_millis(60));
    while let Ok(event) = ring.receiver.try_recv() {
        ring.events.push(event);
    }
    assert_eq!(ring.events.len(), 1);
    assert_eq!(
        std::fs::read_to_string(ring.root.path().join("shared.txt")).unwrap(),
        "1000\n"
    );
}

/// Una CLI incompatibile fallisce senza annunciare un nodo pronto.
#[test]
fn cli_rejects_invalid_configuration_before_starting_a_listener() {
    let _environment = TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
    let output = Command::new(env!("CARGO_BIN_EXE_app_atm"))
        .args([
            "--id",
            "ATM2",
            "--bind",
            "127.0.0.1:7002",
            "--successor",
            "127.0.0.1:7003",
            "--initial-token",
        ])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("vietato"));
    assert!(output.stdout.is_empty());
}

/// Ogni terminale riceve i tre esiti bancari, locali o diffusi dagli altri ATM.
#[test]
fn tutte_le_console_vedono_le_transazioni_tramite_udp() {
    let mut ring = Ring::new();
    for (i, tx) in [
        (3, vec!["withdraw:500"]),
        (2, vec!["deposit:100"]),
        (1, vec!["withdraw:200"]),
        (0, vec![]),
    ] {
        ring.start(i, &tx, 10);
    }
    ring.until(|events| {
        (1..=4).all(|console| {
            (2..=4).all(|source| {
                events.iter().any(|event| {
                    let record = if event["event"] == "remote_log" {
                        &event["source"]
                    } else {
                        event
                    };
                    event["console"] == console
                        && record["event"] == "transaction_finished"
                        && record["atm"] == format!("ATM{source}")
                })
            })
        })
    });
    assert_eq!(
        app_atm::BAccount::read(&ring.root.path().join("shared.txt")).unwrap(),
        400
    );
}

/// Un secondo avvio del bootstrap non può sovrascrivere un file bancario esistente.
#[test]
fn bootstrap_rifiuta_un_conto_gia_esistente() {
    let mut ring = Ring::new();
    let path = ring.root.path().join("shared.txt");
    std::fs::write(&path, "400\n").unwrap();
    ring.reservations[0].take();
    ring.udp_reservations[0].take();
    let output = Command::new(env!("CARGO_BIN_EXE_app_atm"))
        .args([
            "--id",
            "1",
            "--initial-token",
            "--startup-delay-ms",
            "0",
            "--json",
            "--bind",
            &format!("127.0.0.1:{}", ring.base),
            "--successor",
            &format!("127.0.0.1:{}", ring.base + PORT_OFFSETS[1]),
            "--log-base-port",
            &ring.base.to_string(),
            "--account-file",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("token_created"));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "400\n");
}
